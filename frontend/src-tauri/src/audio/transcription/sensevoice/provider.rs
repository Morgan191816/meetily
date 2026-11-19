//! SenseVoice 转写 provider：懒加载识别器，把阻塞的模型加载与解码隔离到阻塞线程池。

use crate::audio::transcription::provider::{
    TranscriptResult, TranscriptionError, TranscriptionProvider,
};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 16kHz 下 100ms 音频；更短的片段没有识别价值。
const MIN_SAMPLES: usize = 1600;

pub struct SenseVoiceProvider {
    paths: Option<PathBuf>,
    recognizer: Arc<Mutex<Option<sherpa_onnx::OfflineRecognizer>>>,
}

pub(crate) fn map_language(pref: Option<&str>) -> &'static str {
    match pref.unwrap_or("auto") {
        "zh" | "zh-CN" | "zh-TW" => "zh",
        "en" | "en-US" | "en-GB" => "en",
        "ja" => "ja",
        "ko" => "ko",
        "yue" => "yue",
        _ => "auto", // "auto" / "auto-translate" / 不支持的语言
    }
}

impl SenseVoiceProvider {
    /// 只记录模型目录；识别器在第一次 transcribe 时创建（模型 228MB，不在启动时加载）。
    pub fn new(dir: PathBuf) -> Self {
        Self {
            paths: Some(dir),
            recognizer: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    pub fn without_model() -> Self {
        Self {
            paths: None,
            recognizer: Arc::new(Mutex::new(None)),
        }
    }

    /// 预加载识别器（只创建，不解码）。供录音开始前调用，避免 228MB 加载发生在录音中途。
    pub async fn ensure_loaded(&self) -> Result<(), TranscriptionError> {
        // 与 transcribe 复用同一把锁，保证识别器只创建一次。
        let mut guard = self.recognizer.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let dir = self.paths.as_ref().ok_or(TranscriptionError::ModelNotLoaded)?;
        let config = super::build_recognizer_config(
            &dir.join(super::model::MODEL_FILE),
            &dir.join(super::model::TOKENS_FILE),
            None,
        );
        let recognizer = tokio::task::spawn_blocking(move || {
            sherpa_onnx::OfflineRecognizer::create(&config)
        })
        .await
        .map_err(|e| TranscriptionError::EngineFailed(format!("识别任务失败：{e}")))?
        .ok_or_else(|| TranscriptionError::EngineFailed("SenseVoice 识别器创建失败".into()))?;
        *guard = Some(recognizer);
        Ok(())
    }
}

#[async_trait]
impl TranscriptionProvider for SenseVoiceProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<TranscriptResult, TranscriptionError> {
        if audio.len() < MIN_SAMPLES {
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: MIN_SAMPLES,
            });
        }
        let dir = self.paths.clone().ok_or(TranscriptionError::ModelNotLoaded)?;
        let lang = map_language(language.as_deref());
        let recognizer = self.recognizer.clone();
        // 模型加载与 decode 都是同步阻塞操作；放进 spawn_blocking，不占用 async 线程，
        // 也不依赖 multi-thread runtime（block_in_place 在 current-thread runtime 会 panic）。
        let text = tokio::task::spawn_blocking(move || {
            let mut guard = recognizer.blocking_lock();
            if guard.is_none() {
                let config = super::build_recognizer_config(
                    &dir.join(super::model::MODEL_FILE),
                    &dir.join(super::model::TOKENS_FILE),
                    Some(lang),
                );
                *guard = Some(
                    sherpa_onnx::OfflineRecognizer::create(&config).ok_or_else(|| {
                        TranscriptionError::EngineFailed("SenseVoice 识别器创建失败".into())
                    })?,
                );
            }
            let recognizer = guard.as_ref().unwrap();
            let stream = recognizer.create_stream();
            stream.accept_waveform(16000, &audio);
            recognizer.decode(&stream);
            Ok::<String, TranscriptionError>(
                stream
                    .get_result()
                    .map(|r| r.text.trim().to_string())
                    .unwrap_or_default(),
            )
        })
        .await
        .map_err(|e| TranscriptionError::EngineFailed(format!("识别任务失败：{e}")))??;

        Ok(TranscriptResult {
            text,
            confidence: None, // SenseVoice 不返回置信度
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        self.recognizer.lock().await.is_some()
    }

    async fn get_current_model(&self) -> Option<String> {
        Some("sense-voice-int8".to_string())
    }

    fn provider_name(&self) -> &'static str {
        "sensevoice"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_language_preferences_to_sense_voice_codes() {
        assert_eq!(map_language(None), "auto");
        assert_eq!(map_language(Some("auto")), "auto");
        assert_eq!(map_language(Some("auto-translate")), "auto");
        assert_eq!(map_language(Some("zh")), "zh");
        assert_eq!(map_language(Some("en-US")), "en");
        assert_eq!(map_language(Some("de")), "auto"); // 不支持的语言回退自动
    }

    #[test]
    fn rejects_audio_shorter_than_minimum() {
        let provider = SenseVoiceProvider::without_model();
        let err = futures::executor::block_on(provider.transcribe(vec![0.0; 100], None)).unwrap_err();
        assert!(matches!(err, TranscriptionError::AudioTooShort { .. }));
    }

    #[test]
    fn reports_model_not_loaded_before_ensure() {
        let provider = SenseVoiceProvider::without_model();
        assert!(!futures::executor::block_on(provider.is_model_loaded()));
    }

    #[test]
    fn ensure_loaded_without_model_reports_model_not_loaded() {
        let provider = SenseVoiceProvider::without_model();
        let err = futures::executor::block_on(provider.ensure_loaded()).unwrap_err();
        assert!(matches!(err, TranscriptionError::ModelNotLoaded));
    }
}
