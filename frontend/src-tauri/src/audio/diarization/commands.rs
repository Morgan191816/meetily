//! 说话人相关的 Tauri 命令：列表、重命名、OA 名单、模型管理。

use super::model::{
    base_url, inspect_models, model_dir, DiarizationModelStatus, EMBEDDING_FILE,
    EMBEDDING_SHA256, EMBEDDING_SIZE, SEGMENTATION_FILE, SEGMENTATION_SHA256, SEGMENTATION_SIZE,
};
use super::runner::{cancel_run, run_guarded};
use super::store::{list_speakers, set_display_name, SpeakerRow};
use crate::audio::transcription::sensevoice::model::{cleanup_partials, download_file};
use crate::state::AppState;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{command, AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

// 只用 State 的命令不引入 `R: Runtime`：State 无法约束 R，generate_handler 会 E0283
#[command]
pub async fn api_get_meeting_speakers(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<SpeakerRow>, String> {
    list_speakers(state.db_manager.pool(), &meeting_id).await
}

#[command]
pub async fn api_rename_speaker(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    label: String,
    name: String,
) -> Result<(), String> {
    set_display_name(state.db_manager.pool(), &meeting_id, &label, &name).await
}

#[command]
pub async fn api_get_roster_names() -> Result<Vec<String>, String> {
    Ok(crate::minutes::roster::member_names().to_vec())
}

struct ActiveDownload {
    token: CancellationToken,
    progress: Arc<AtomicU8>,
}

// 与 SenseVoice 的下载槽位分开：两套模型互不影响
static ACTIVE: Mutex<Option<ActiveDownload>> = Mutex::new(None);

fn app_data<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败：{e}"))
}

fn active_progress() -> Option<u8> {
    ACTIVE
        .lock()
        .unwrap()
        .as_ref()
        .map(|d| d.progress.load(Ordering::Relaxed))
}

/// 两个文件合并为一条进度：segmentation 占 0–40%，embedding 占 40–100%。
fn overall_progress(file_index: u8, percent: u8) -> u8 {
    let percent = percent.min(100) as u16;
    match file_index {
        0 => (percent * 40 / 100) as u8,
        _ => (40 + percent * 60 / 100) as u8,
    }
}

#[command]
pub async fn diarization_get_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<DiarizationModelStatus, String> {
    let dir = model_dir(&app_data(&app)?);
    if let Some(progress) = active_progress() {
        return Ok(DiarizationModelStatus {
            state: "downloading",
            progress: Some(progress),
            path: dir.to_string_lossy().into_owned(),
        });
    }
    Ok(inspect_models(&dir))
}

#[command]
pub async fn diarization_download_models<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = model_dir(&app_data(&app)?);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建模型目录失败：{e}"))?;

    let token = CancellationToken::new();
    let progress = Arc::new(AtomicU8::new(0));
    {
        let mut active = ACTIVE.lock().unwrap();
        if active.is_some() {
            return Err("模型正在下载中".to_string());
        }
        *active = Some(ActiveDownload { token: token.clone(), progress: progress.clone() });
    }

    let mut last_reported: Option<u8> = None;
    let mut report = |percent: u8| {
        progress.store(percent, Ordering::Relaxed);
        if last_reported != Some(percent) {
            last_reported = Some(percent);
            let _ = app.emit("diarization-download-progress", serde_json::json!({ "progress": percent }));
        }
    };

    let result = async {
        download_file(
            &format!("{}/{SEGMENTATION_FILE}", base_url()),
            &dir.join(SEGMENTATION_FILE),
            SEGMENTATION_SIZE,
            SEGMENTATION_SHA256,
            &token,
            |p| report(overall_progress(0, p)),
        )
        .await?;
        download_file(
            &format!("{}/{EMBEDDING_FILE}", base_url()),
            &dir.join(EMBEDDING_FILE),
            EMBEDDING_SIZE,
            EMBEDDING_SHA256,
            &token,
            |p| report(overall_progress(1, p)),
        )
        .await
    }
    .await;

    // 删除后可能已开始新一轮下载：只有槽位仍属于本任务时才收尾
    let mut active = ACTIVE.lock().unwrap();
    let still_owner = active
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(&current.progress, &progress));
    if still_owner {
        *active = None;
    }
    drop(active);

    match result {
        Ok(()) => {
            let _ = app.emit("diarization-download-complete", serde_json::json!({}));
            Ok(())
        }
        Err(_) if token.is_cancelled() => {
            if still_owner {
                cleanup_partials(&dir);
            }
            Ok(())
        }
        Err(error) => {
            if still_owner {
                cleanup_partials(&dir);
                let _ = app.emit("diarization-download-error", serde_json::json!({ "error": error }));
            }
            Err(error)
        }
    }
}

#[command]
pub async fn diarization_cancel_download<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let token = ACTIVE.lock().unwrap().as_ref().map(|d| d.token.clone());
    if let Some(token) = token {
        token.cancel();
        cleanup_partials(&model_dir(&app_data(&app)?));
    }
    Ok(())
}

#[command]
pub async fn diarization_delete_models<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if let Some(active) = ACTIVE.lock().unwrap().take() {
        active.token.cancel();
    }
    let dir = model_dir(&app_data(&app)?);
    for file in [SEGMENTATION_FILE, EMBEDDING_FILE] {
        let path = dir.join(file);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("删除模型文件失败：{e}"))?;
        }
    }
    cleanup_partials(&dir);
    Ok(())
}

#[command]
pub async fn api_run_diarization<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<usize, String> {
    run_guarded(app, state.db_manager.pool().clone(), meeting_id)
        .await
        .map(|outcome| outcome.labels)
}

#[command]
pub async fn api_cancel_diarization(meeting_id: String) -> Result<bool, String> {
    Ok(cancel_run(&meeting_id))
}

#[cfg(test)]
mod tests {
    use super::overall_progress;

    #[test]
    fn progress_splits_between_files_and_clamps() {
        assert_eq!(overall_progress(0, 0), 0);
        assert_eq!(overall_progress(0, 100), 40);
        assert_eq!(overall_progress(1, 0), 40);
        assert_eq!(overall_progress(1, 100), 100);
        assert_eq!(overall_progress(1, 50), 70);
        assert_eq!(overall_progress(0, 200), 40);
    }
}
