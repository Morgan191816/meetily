//! 分离运行器：解码 → sherpa 处理 → 对齐 → 写库。

use super::align::{assign_speakers, label_for, DiarSegment};
use super::model::{inspect_models, model_dir, EMBEDDING_FILE, SEGMENTATION_FILE};
use crate::audio::decoder::decode_audio_file;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

pub struct DiarizationOutcome {
    pub labels: usize,
}

/// 在途分离任务，`cancel_run` 按 meeting_id 取消。
static ACTIVE_RUNS: LazyLock<Mutex<HashMap<String, CancellationToken>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 排队等槽位的任务；取消它与取消在跑任务等效。
static PENDING_RUNS: LazyLock<Mutex<HashMap<String, Arc<CancellationToken>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 同一时刻只跑一个会议的分离，避免两份解码缓冲同时驻留。
static DIARIZATION_SEMAPHORE: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(1));

/// 旧任务在阶段边界才响应取消，`diarize_blocking` 不可中断：
/// 2 小时会议要跑 10–30 分钟，排队必须等得起。
const QUEUE_POLL: Duration = Duration::from_millis(500);
const QUEUE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// 离开作用域即释放槽位：`run_for_meeting` 返回 Err 或 panic 都不会卡住该会议。
struct RunGuard(String);

impl Drop for RunGuard {
    fn drop(&mut self) {
        if let Ok(mut runs) = ACTIVE_RUNS.lock() {
            runs.remove(&self.0);
        }
    }
}

/// 抢占会议槽位；该会议已在分离时返回 Err。
fn claim(meeting_id: &str) -> Result<CancellationToken, String> {
    let mut runs = ACTIVE_RUNS.lock().unwrap();
    if runs.contains_key(meeting_id) {
        return Err("该会议正在区分说话人".to_string());
    }
    let token = CancellationToken::new();
    runs.insert(meeting_id.to_string(), token.clone());
    Ok(token)
}

/// 登记排队令牌 → 等槽位 → 运行 → 释放令牌。手动与自动共用：旧任务不可中断时排队而非拒绝。
pub(crate) async fn run_queued<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    meeting_id: String,
) -> Result<DiarizationOutcome, String> {
    let pending = Arc::new(CancellationToken::new());
    if let Ok(mut runs) = PENDING_RUNS.lock() {
        // 覆盖同会议旧令牌前先取消它，避免两个排队者抢跑
        if let Some(previous) = runs.insert(meeting_id.clone(), pending.clone()) {
            previous.cancel();
        }
    }
    let result = run_when_free(app, pool, meeting_id.clone(), pending.clone()).await;
    // 只有仍是自己的令牌才移除，别把新一轮的排队抹掉
    if let Ok(mut runs) = PENDING_RUNS.lock() {
        if runs
            .get(&meeting_id)
            .is_some_and(|token| Arc::ptr_eq(token, &pending))
        {
            runs.remove(&meeting_id);
        }
    }
    result
}

/// 取消在途分离或排队中的任务；两者都没有时返回 false。
pub(crate) fn cancel_run(meeting_id: &str) -> bool {
    let active = ACTIVE_RUNS
        .lock()
        .unwrap()
        .get(meeting_id)
        .map(|token| token.cancel());
    let pending = PENDING_RUNS
        .lock()
        .unwrap()
        .get(meeting_id)
        .map(|token| token.cancel());
    active.is_some() || pending.is_some()
}

/// 模型就绪时后台分离；缺失时发一次性提示。三处触发点共用。
pub fn spawn_auto_diarization<R: Runtime>(app: AppHandle<R>, pool: SqlitePool, meeting_id: String) {
    let ready = app
        .path()
        .app_data_dir()
        .map(|data| inspect_models(&model_dir(&data)).state == "available")
        .unwrap_or(false);
    if !ready {
        log::info!("说话人模型未安装，跳过自动分离：meeting_id={meeting_id}");
        let _ = app.emit(
            "diarization-unavailable",
            serde_json::json!({ "meetingId": meeting_id }),
        );
        return;
    }
    tauri::async_runtime::spawn(async move {
        let id = meeting_id.clone();
        if let Err(error) = run_queued(app.clone(), pool, meeting_id).await {
            log::warn!("自动说话人分离失败：meeting_id={id}, error={error}");
            // 取消不算失败：前端已自行复位，不弹错误
            if !error.contains("已取消") {
                let _ = app.emit(
                    "diarization-error",
                    serde_json::json!({ "meetingId": id, "error": error }),
                );
            }
        }
    });
}

/// 等槽位释放后再跑：旧任务可能还要跑 10–30 分钟。
/// 超过 `QUEUE_TIMEOUT` 才放弃；排队期间被取消则直接放弃。
async fn run_when_free<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    meeting_id: String,
    pending: Arc<CancellationToken>,
) -> Result<DiarizationOutcome, String> {
    let mut waited = Duration::ZERO;
    while waited < QUEUE_TIMEOUT {
        if pending.is_cancelled() {
            return Err("已取消".to_string());
        }
        if let Ok(token) = claim(&meeting_id) {
            let _guard = RunGuard(meeting_id.clone());
            return run_for_meeting(app, pool, meeting_id, token).await;
        }
        tokio::time::sleep(QUEUE_POLL).await;
        waited += QUEUE_POLL;
    }
    Err("该会议正在区分说话人".to_string())
}

fn build_config(dir: &Path) -> sherpa_onnx::OfflineSpeakerDiarizationConfig {
    let mut config = sherpa_onnx::OfflineSpeakerDiarizationConfig::default();
    config.segmentation.pyannote.model =
        Some(dir.join(SEGMENTATION_FILE).to_string_lossy().into_owned());
    config.embedding.model = Some(dir.join(EMBEDDING_FILE).to_string_lossy().into_owned());
    config.clustering.num_clusters = -1;
    config.clustering.threshold = 0.5;
    config
}

fn diarize_blocking(audio: Vec<f32>, dir: PathBuf) -> Result<Vec<DiarSegment>, String> {
    let diarizer = sherpa_onnx::OfflineSpeakerDiarization::create(&build_config(&dir))
        .ok_or("说话人分离模型加载失败")?;
    let result = diarizer.process(&audio).ok_or("说话人分离失败")?;
    Ok(result
        .sort_by_start_time()
        .into_iter()
        .map(|segment| DiarSegment {
            start: segment.start,
            end: segment.end,
            speaker: segment.speaker,
        })
        .collect())
}

fn resolve_audio_path(folder: &Path) -> Result<PathBuf, String> {
    // metadata.json 的 audio_file 优先；缺失时在目录里找第一个受支持的音频文件
    let named = std::fs::read_to_string(folder.join("metadata.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|value| {
            value
                .get("audio_file")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
    if let Some(name) = named {
        // 只接受目录内的单个文件名：`..` 或绝对路径可读到会议目录外
        let candidate = Path::new(&name);
        if candidate.components().count() == 1 && !candidate.is_absolute() {
            let path = folder.join(&name);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(folder)
        .map_err(|e| format!("读取录音目录失败：{e}"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("wav" | "mp3" | "mp4" | "m4a" | "flac" | "ogg")
            )
        })
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| "录音目录里没有音频文件".to_string())
}

/// 事务内写入标签；取消时整批放弃。重跑先清空该会议旧标签，避免编号重排后张冠李戴。
pub async fn write_assignments(
    pool: &SqlitePool,
    meeting_id: &str,
    rows: &[(String, Option<f64>, Option<f64>)],
    assignments: &[Option<usize>],
) -> Result<usize, String> {
    let mut tx = pool.begin().await.map_err(|e| format!("开启事务失败：{e}"))?;
    sqlx::query("UPDATE transcripts SET speaker = NULL WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("清除旧说话人失败：{e}"))?;
    let mut labels = std::collections::BTreeSet::new();
    for ((id, _, _), assignment) in rows.iter().zip(assignments.iter()) {
        let Some(index) = assignment else { continue };
        let label = label_for(*index);
        labels.insert(label.clone());
        sqlx::query("UPDATE transcripts SET speaker = ? WHERE id = ?")
            .bind(&label)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("写入说话人失败：{e}"))?;
    }
    tx.commit().await.map_err(|e| format!("提交失败：{e}"))?;
    Ok(labels.len())
}

pub async fn run_for_meeting<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    meeting_id: String,
    cancel: CancellationToken,
) -> Result<DiarizationOutcome, String> {
    let progress = |stage: &str, percent: u8| {
        let _ = app.emit(
            "diarization-progress",
            serde_json::json!({ "meetingId": meeting_id, "stage": stage, "percent": percent }),
        );
    };

    let folder: Option<String> =
        sqlx::query_scalar::<_, Option<String>>("SELECT folder_path FROM meetings WHERE id = ?")
            .bind(&meeting_id)
            .fetch_one(&pool)
            .await
            .map_err(|e| format!("读取会议失败：{e}"))?;
    let folder = PathBuf::from(folder.ok_or("会议没有录音目录")?);
    let audio_path = resolve_audio_path(&folder)?;

    if cancel.is_cancelled() {
        return Err("已取消".to_string());
    }
    // 全局串行：解码缓冲与 sherpa 模型都很吃内存，等槽位期间可取消。
    let _permit = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err("已取消".to_string()),
        permit = DIARIZATION_SEMAPHORE.acquire() => {
            permit.map_err(|error| format!("等待分离槽位失败：{error}"))?
        }
    };
    progress("decoding", 5);
    let samples = tokio::task::spawn_blocking(move || {
        decode_audio_file(&audio_path).map(|decoded| decoded.to_whisper_format())
    })
    .await
    .map_err(|e| format!("解码任务失败：{e}"))?
    .map_err(|e| format!("解码音频失败：{e}"))?;

    if cancel.is_cancelled() {
        return Err("已取消".to_string());
    }
    progress("diarizing", 30);
    let dir = model_dir(
        &app.path()
            .app_data_dir()
            .map_err(|e| format!("获取应用数据目录失败：{e}"))?,
    );
    let segments = tokio::task::spawn_blocking(move || diarize_blocking(samples, dir))
        .await
        .map_err(|e| format!("分离任务失败：{e}"))??;

    if cancel.is_cancelled() {
        return Err("已取消".to_string());
    }
    progress("writing", 85);
    let rows = sqlx::query_as::<_, (String, Option<f64>, Option<f64>)>(
        "SELECT id, audio_start_time, audio_end_time FROM transcripts WHERE meeting_id = ?
         ORDER BY COALESCE(audio_start_time, 0) ASC, rowid ASC",
    )
    .bind(&meeting_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| format!("读取逐字稿失败：{e}"))?;

    let times: Vec<(Option<f64>, Option<f64>)> = rows.iter().map(|(_, s, e)| (*s, *e)).collect();
    let assignments = assign_speakers(&times, &segments);
    let labels = write_assignments(&pool, &meeting_id, &rows, &assignments).await?;

    progress("done", 100);
    let _ = app.emit(
        "speakers-updated",
        serde_json::json!({ "meetingId": meeting_id, "labels": labels }),
    );
    Ok(DiarizationOutcome { labels })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE transcripts (id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, speaker TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for id in ["t1", "t2", "t3"] {
            sqlx::query("INSERT INTO transcripts (id, meeting_id) VALUES (?, 'm1')")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    async fn speakers(pool: &SqlitePool) -> Vec<Option<String>> {
        sqlx::query_scalar::<_, Option<String>>("SELECT speaker FROM transcripts ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[test]
    fn claim_rejects_second_run_and_guard_frees_slot() {
        let id = "test-claim-meeting";
        let token = claim(id).unwrap();
        assert!(claim(id).is_err(), "同一会议不应并发抢占槽位");
        drop(RunGuard(id.to_string()));
        assert!(!token.is_cancelled(), "抢占失败不应取消在跑任务");
        assert!(claim(id).is_ok(), "槽位释放后应可再次抢占");
        drop(RunGuard(id.to_string()));
    }

    #[tokio::test]
    async fn global_semaphore_allows_only_one_diarization() {
        let permit = DIARIZATION_SEMAPHORE.acquire().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), DIARIZATION_SEMAPHORE.acquire())
                .await
                .is_err(),
            "第二个会议必须等第一个释放槽位"
        );
        drop(permit);
        assert!(DIARIZATION_SEMAPHORE.try_acquire().is_ok());
    }

    #[tokio::test]
    async fn writes_labels_and_leaves_unassigned_null() {
        let pool = test_pool().await;
        let rows = vec![
            ("t1".to_string(), Some(0.0), Some(1.0)),
            ("t2".to_string(), Some(1.0), Some(2.0)),
            ("t3".to_string(), Some(2.0), Some(3.0)),
        ];
        let labels = write_assignments(&pool, "m1", &rows, &[Some(0), Some(1), None])
            .await
            .unwrap();
        assert_eq!(labels, 2);

        assert_eq!(
            speakers(&pool).await,
            vec![
                Some("SPEAKER_00".to_string()),
                Some("SPEAKER_01".to_string()),
                None,
            ]
        );
    }

    #[tokio::test]
    async fn rerun_clears_previous_labels() {
        let pool = test_pool().await;
        let rows = vec![
            ("t1".to_string(), Some(0.0), Some(1.0)),
            ("t2".to_string(), Some(1.0), Some(2.0)),
            ("t3".to_string(), Some(2.0), Some(3.0)),
        ];
        write_assignments(&pool, "m1", &rows, &[Some(0), Some(1), Some(0)])
            .await
            .unwrap();

        // 第二轮未对齐的行不能保留上一轮标签：编号会按首次出现重排
        let labels = write_assignments(&pool, "m1", &rows, &[Some(1), None, None])
            .await
            .unwrap();
        assert_eq!(labels, 1);
        assert_eq!(
            speakers(&pool).await,
            vec![Some("SPEAKER_01".to_string()), None, None]
        );
    }

    #[test]
    fn ignores_audio_file_outside_folder() {
        let base = std::env::temp_dir().join(format!("diarization-resolve-{}", std::process::id()));
        let folder = base.join("meeting");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("metadata.json"),
            r#"{"audio_file":"../escape.wav"}"#,
        )
        .unwrap();
        std::fs::write(base.join("escape.wav"), b"x").unwrap();
        std::fs::write(folder.join("real.wav"), b"x").unwrap();

        let resolved = resolve_audio_path(&folder).unwrap();
        assert_eq!(resolved.file_name().unwrap(), "real.wav");

        std::fs::remove_dir_all(&base).unwrap();
    }
}
