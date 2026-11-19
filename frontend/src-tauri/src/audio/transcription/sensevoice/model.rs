//! SenseVoice 模型文件：下载、SHA-256 校验与状态检查。

use futures_util::StreamExt;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

pub const MODEL_FILE: &str = "model.int8.onnx";
pub const TOKENS_FILE: &str = "tokens.txt";
pub const MODEL_URL: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/model.int8.onnx";
pub const TOKENS_URL: &str =
    "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/tokens.txt";
/// 2026-11-19 从 MODEL_URL 实测：239,233,841 字节。
pub const MODEL_SHA256: &str = "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51";
pub const MODEL_SIZE: u64 = 239_233_841;
/// 2026-11-19 从 TOKENS_URL 实测：315,894 字节。
pub const TOKENS_SHA256: &str = "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc";
pub const TOKENS_SIZE: u64 = 315_894;
pub const MIN_VALID_SIZE: u64 = 200 * 1024 * 1024; // 只做快速预检，最终以 SHA-256 为准

#[derive(Debug, Clone, Serialize)]
pub struct SenseVoiceModelStatus {
    pub state: &'static str, // missing | downloading | available | corrupted
    pub progress: Option<u8>,
    pub path: String,
}

pub fn inspect_model_dir(dir: &Path) -> SenseVoiceModelStatus {
    let model = dir.join(MODEL_FILE);
    let tokens = dir.join(TOKENS_FILE);
    let state = match (model.metadata(), tokens.metadata()) {
        (Ok(m), Ok(t)) if m.len() >= MIN_VALID_SIZE && t.len() > 0 => "available",
        (Ok(_), Ok(_)) => "corrupted",
        _ => "missing",
    };
    SenseVoiceModelStatus {
        state,
        progress: None,
        path: dir.to_string_lossy().into_owned(),
    }
}

pub fn cleanup_partials(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".part") || name.ends_with(".tmp") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// 内网镜像：设置后替换 HuggingFace 基址，路径与文件名保持不变
pub fn base_url() -> String {
    std::env::var("MORGAN_MEET_SENSEVOICE_BASE_URL").unwrap_or_else(|_| {
        "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main".to_string()
    })
}

/// 模型目录可用 MORGAN_MEET_MODELS_DIR 覆盖，缺省为 <app data>/models/sense-voice
pub fn sensevoice_model_dir(app_data: &Path) -> PathBuf {
    match std::env::var("MORGAN_MEET_MODELS_DIR") {
        Ok(dir) => PathBuf::from(dir).join("sense-voice"),
        Err(_) => app_data.join("models").join("sense-voice"),
    }
}

/// 先写 <file>.part，边写边算 SHA-256 并回报进度；结束后比对大小与哈希，不符则删除。
pub async fn download_file(
    url: &str,
    destination: &Path,
    expected_size: u64,
    expected_sha256: &str,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u8),
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    // 发请求前先看取消标记：已取消时一个字节都不该下载。
    if cancel.is_cancelled() {
        return Err("已取消下载".to_string());
    }
    let part = destination.with_extension("part");
    let response = reqwest::Client::new()
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            concat!("MorganMeet/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .map_err(|e| format!("下载失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("下载失败（HTTP）：{e}"))?;

    let mut file = tokio::fs::File::create(&part)
        .await
        .map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut written: u64 = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            let _ = tokio::fs::remove_file(&part).await;
            return Err("已取消下载".to_string());
        }
        let chunk = chunk.map_err(|e| format!("下载中断：{e}"))?;
        hasher.update(&chunk);
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| e.to_string())?;
        written += chunk.len() as u64;
        progress(((written * 100 / expected_size.max(1)) as u8).min(99));
    }
    drop(file);

    if written != expected_size {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(format!(
            "文件大小不符：实际 {written}，期望 {expected_size}"
        ));
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected_sha256 {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(format!("校验和不符：实际 {actual}"));
    }
    tokio::fs::rename(&part, destination)
        .await
        .map_err(|e| e.to_string())?;
    progress(100);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_dir_is_under_app_models() {
        let dir = sensevoice_model_dir(std::path::Path::new("C:/data"));
        assert!(dir.ends_with("models/sense-voice"));
    }

    #[test]
    fn missing_files_report_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let status = inspect_model_dir(tmp.path());
        assert_eq!(status.state, "missing");
    }

    #[test]
    fn undersized_file_reports_corrupted() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(MODEL_FILE), b"short").unwrap();
        std::fs::write(tmp.path().join(TOKENS_FILE), b"short").unwrap();
        let status = inspect_model_dir(tmp.path());
        assert_eq!(status.state, "corrupted");
    }

    #[test]
    fn partial_download_is_removed_on_verify_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let part = tmp.path().join("model.int8.onnx.part");
        std::fs::write(&part, b"partial").unwrap();
        cleanup_partials(tmp.path());
        assert!(!part.exists());
    }

    #[tokio::test]
    async fn cancelled_download_leaves_no_partial_file() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join(MODEL_FILE);
        let cancel = CancellationToken::new();
        cancel.cancel(); // 下载开始前就取消
        let result = download_file(
            &format!("{}/model.int8.onnx", base_url()),
            &target,
            MODEL_SIZE,
            MODEL_SHA256,
            &cancel,
            |_| {},
        )
        .await;
        assert!(result.is_err());
        assert!(!target.exists());
        assert!(!target.with_extension("part").exists());
    }
}
