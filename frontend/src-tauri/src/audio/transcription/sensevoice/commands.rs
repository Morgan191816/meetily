//! SenseVoice 模型的 Tauri 命令：状态、下载、取消、删除。

use super::model::{
    base_url, cleanup_partials, download_file, inspect_model_dir, sensevoice_model_dir,
    SenseVoiceModelStatus, MODEL_FILE, MODEL_SHA256, MODEL_SIZE, TOKENS_FILE, TOKENS_SHA256,
    TOKENS_SIZE,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{command, AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

struct ActiveDownload {
    token: CancellationToken,
    progress: Arc<AtomicU8>,
}

/// 键为模型文件名；只有一个 SenseVoice 模型，但保留键位便于将来扩展。
static DOWNLOADS: Mutex<HashMap<&'static str, ActiveDownload>> = Mutex::new(HashMap::new());

fn model_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败：{e}"))?;
    Ok(sensevoice_model_dir(&data))
}

#[command]
pub async fn sensevoice_get_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<SenseVoiceModelStatus, String> {
    let dir = model_dir(&app)?;
    let progress = DOWNLOADS
        .lock()
        .unwrap()
        .get(MODEL_FILE)
        .map(|d| d.progress.load(Ordering::Relaxed));
    if let Some(progress) = progress {
        return Ok(SenseVoiceModelStatus {
            state: "downloading",
            progress: Some(progress),
            path: dir.to_string_lossy().into_owned(),
        });
    }
    Ok(inspect_model_dir(&dir))
}

#[command]
pub async fn sensevoice_download_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = model_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建模型目录失败：{e}"))?;

    let token = CancellationToken::new();
    let progress = Arc::new(AtomicU8::new(0));
    {
        let mut downloads = DOWNLOADS.lock().unwrap();
        if downloads.contains_key(MODEL_FILE) {
            return Err("模型正在下载中".to_string());
        }
        downloads.insert(
            MODEL_FILE,
            ActiveDownload {
                token: token.clone(),
                progress: progress.clone(),
            },
        );
    }

    // 每块都发事件会有上万条，只在百分比变化时发
    let mut last_reported: Option<u8> = None;
    let mut on_progress = |percent: u8| {
        progress.store(percent, Ordering::Relaxed);
        if last_reported != Some(percent) {
            last_reported = Some(percent);
            let _ = app.emit(
                "sensevoice-download-progress",
                serde_json::json!({ "progress": percent }),
            );
        }
    };

    let result = async {
        download_file(
            &format!("{}/{MODEL_FILE}", base_url()),
            &dir.join(MODEL_FILE),
            MODEL_SIZE,
            MODEL_SHA256,
            &token,
            &mut on_progress,
        )
        .await?;
        download_file(
            &format!("{}/{TOKENS_FILE}", base_url()),
            &dir.join(TOKENS_FILE),
            TOKENS_SIZE,
            TOKENS_SHA256,
            &token,
            &mut on_progress,
        )
        .await
    }
    .await;

    DOWNLOADS.lock().unwrap().remove(MODEL_FILE);
    match result {
        Ok(()) => {
            let _ = app.emit("sensevoice-download-complete", serde_json::json!({}));
            Ok(())
        }
        // 与 Whisper 一致：取消由取消命令/UI 负责收尾，不在这里发终态事件或报错
        Err(_) if token.is_cancelled() => {
            cleanup_partials(&dir);
            Ok(())
        }
        Err(error) => {
            cleanup_partials(&dir);
            let _ = app.emit(
                "sensevoice-download-error",
                serde_json::json!({ "error": error }),
            );
            Err(error)
        }
    }
}

#[command]
pub async fn sensevoice_cancel_download<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let token = DOWNLOADS
        .lock()
        .unwrap()
        .get(MODEL_FILE)
        .map(|d| d.token.clone());
    if let Some(token) = token {
        token.cancel();
        cleanup_partials(&model_dir(&app)?);
    }
    Ok(())
}

#[command]
pub async fn sensevoice_delete_model<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if let Some(active) = DOWNLOADS.lock().unwrap().remove(MODEL_FILE) {
        active.token.cancel();
    }
    let dir = model_dir(&app)?;
    for file in [MODEL_FILE, TOKENS_FILE] {
        let path = dir.join(file);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("删除模型文件失败：{e}"))?;
        }
    }
    cleanup_partials(&dir);
    Ok(())
}
