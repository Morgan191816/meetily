//! 说话人分离模型：pyannote 分割 + 3D-Speaker 中文 embedding。

use serde::Serialize;
use std::path::{Path, PathBuf};

pub const SEGMENTATION_FILE: &str = "segmentation.onnx";
pub const EMBEDDING_FILE: &str = "embedding.onnx";
pub const SEGMENTATION_SIZE: u64 = 5_992_913;
pub const SEGMENTATION_SHA256: &str =
    "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079";
pub const EMBEDDING_SIZE: u64 = 28_281_139;
pub const EMBEDDING_SHA256: &str =
    "e6af4d4c444f3dab01d991bb9b7d5af98eb979e329f6c8880a19d5553561ed25";

#[derive(Debug, Clone, Serialize)]
pub struct DiarizationModelStatus {
    pub state: &'static str, // missing | downloading | available
    pub progress: Option<u8>,
    pub path: String,
}

pub fn model_dir(app_data: &Path) -> PathBuf {
    match std::env::var("MORGAN_MEET_MODELS_DIR") {
        Ok(dir) => PathBuf::from(dir).join("diarization"),
        Err(_) => app_data.join("models").join("diarization"),
    }
}

/// 下载基址：固定 tag `models` 的 Release，可用环境变量覆盖为内网镜像。
pub fn base_url() -> String {
    std::env::var("MORGAN_MEET_DIARIZATION_BASE_URL").unwrap_or_else(|_| {
        "https://github.com/Morgan191816/meetily/releases/download/models".to_string()
    })
}

pub fn inspect_models(dir: &Path) -> DiarizationModelStatus {
    let ready = [SEGMENTATION_FILE, EMBEDDING_FILE]
        .iter()
        .all(|file| dir.join(file).metadata().map(|m| m.len() > 1024 * 1024).unwrap_or(false));
    DiarizationModelStatus {
        state: if ready { "available" } else { "missing" },
        progress: None,
        path: dir.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_dir_is_under_app_models() {
        assert!(model_dir(Path::new("C:/data")).ends_with("models/diarization"));
    }

    #[test]
    fn empty_dir_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(inspect_models(tmp.path()).state, "missing");
    }
}
