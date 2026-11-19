//! SenseVoice 离线识别：模型管理、provider 实现与 Tauri 命令。

use sherpa_onnx::{OfflineRecognizerConfig, OfflineSenseVoiceModelConfig};
use std::path::Path;

pub mod model;
pub mod provider;

pub const SUPPORTED_LANGUAGES: [&str; 6] = ["auto", "zh", "en", "ja", "ko", "yue"];

/// 构造识别器配置。SenseVoice 只支持 greedy_search，写死避免进程被 SHERPA_ONNX_EXIT 终止。
pub(crate) fn build_recognizer_config(
    model: &Path,
    tokens: &Path,
    language: Option<&str>,
) -> OfflineRecognizerConfig {
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
        model: Some(model.to_string_lossy().into_owned()),
        language: Some(language.unwrap_or("auto").to_string()),
        use_itn: true,
    };
    config.model_config.tokens = Some(tokens.to_string_lossy().into_owned());
    config.model_config.provider = Some("cpu".to_string());
    config.model_config.num_threads = 4;
    config.model_config.debug = false;
    config.decoding_method = Some("greedy_search".to_string());
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_languages_include_auto_and_chinese() {
        assert!(SUPPORTED_LANGUAGES.contains(&"auto"));
        assert!(SUPPORTED_LANGUAGES.contains(&"zh"));
        assert_eq!(SUPPORTED_LANGUAGES.len(), 6);
    }

    #[test]
    fn default_recognizer_config_is_greedy_and_cpu() {
        let config = build_recognizer_config(
            std::path::Path::new("model.int8.onnx"),
            std::path::Path::new("tokens.txt"),
            Some("auto"),
        );
        assert_eq!(config.decoding_method.as_deref(), Some("greedy_search"));
        assert_eq!(config.model_config.provider.as_deref(), Some("cpu"));
        assert_eq!(config.model_config.num_threads, 4);
        assert!(config.model_config.sense_voice.use_itn);
    }
}
