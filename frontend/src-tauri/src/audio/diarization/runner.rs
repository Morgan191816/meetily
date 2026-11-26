//! 分离运行器：解码 → sherpa 处理 → 对齐 → 写库。

use super::align::{assign_speakers, label_for, DiarSegment};
use super::model::{inspect_models, model_dir, EMBEDDING_FILE, SEGMENTATION_FILE};
use crate::audio::decoder::decode_audio_file;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

pub struct DiarizationOutcome {
    pub labels: usize,
}

/// 在途分离任务，`cancel_run` 按 meeting_id 取消。
static ACTIVE_RUNS: LazyLock<Mutex<HashMap<String, CancellationToken>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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

/// 注册 → 运行 → 移除。命令路径与自动触发共用，同一会议不并发分离。
pub(crate) async fn run_guarded<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    meeting_id: String,
) -> Result<DiarizationOutcome, String> {
    let token = claim(&meeting_id)?;
    let _guard = RunGuard(meeting_id.clone());
    run_for_meeting(app, pool, meeting_id, token).await
}

/// 取消在途分离；没有在跑的任务返回 false。
pub(crate) fn cancel_run(meeting_id: &str) -> bool {
    match ACTIVE_RUNS.lock().unwrap().get(meeting_id) {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    }
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
        if let Err(error) = run_guarded(app, pool, meeting_id).await {
            log::warn!("自动说话人分离失败：meeting_id={id}, error={error}");
        }
    });
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
        let path = folder.join(&name);
        if path.is_file() {
            return Ok(path);
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

/// 事务内写入标签；取消时整批放弃。
pub async fn write_assignments(
    pool: &SqlitePool,
    rows: &[(String, Option<f64>, Option<f64>)],
    assignments: &[Option<usize>],
) -> Result<usize, String> {
    let mut tx = pool.begin().await.map_err(|e| format!("开启事务失败：{e}"))?;
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
    let labels = write_assignments(&pool, &rows, &assignments).await?;

    progress("done", 100);
    let _ = app.emit("speakers-updated", serde_json::json!({ "meetingId": meeting_id }));
    Ok(DiarizationOutcome { labels })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE transcripts (id TEXT PRIMARY KEY, speaker TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        for id in ["t1", "t2", "t3"] {
            sqlx::query("INSERT INTO transcripts (id) VALUES (?)")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
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
    async fn writes_labels_and_leaves_unassigned_null() {
        let pool = test_pool().await;
        let rows = vec![
            ("t1".to_string(), Some(0.0), Some(1.0)),
            ("t2".to_string(), Some(1.0), Some(2.0)),
            ("t3".to_string(), Some(2.0), Some(3.0)),
        ];
        let labels = write_assignments(&pool, &rows, &[Some(0), Some(1), None])
            .await
            .unwrap();
        assert_eq!(labels, 2);

        let speakers: Vec<Option<String>> =
            sqlx::query_scalar::<_, Option<String>>("SELECT speaker FROM transcripts ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            speakers,
            vec![
                Some("SPEAKER_00".to_string()),
                Some("SPEAKER_01".to_string()),
                None,
            ]
        );
    }
}
