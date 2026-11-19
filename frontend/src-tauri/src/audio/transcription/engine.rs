// audio/transcription/engine.rs
//
// TranscriptionEngine enum and model initialization/validation logic.

use super::provider::TranscriptionProvider;
use super::sensevoice::provider::SenseVoiceProvider;
use log::{info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime};

// ============================================================================
// TRANSCRIPTION ENGINE ENUM
// ============================================================================

// Transcription engine abstraction to support multiple providers
pub enum TranscriptionEngine {
    Whisper(Arc<crate::whisper_engine::WhisperEngine>),  // Direct access (backward compat)
    Provider(Arc<dyn TranscriptionProvider>),  // Trait-based (preferred for new code)
}

impl TranscriptionEngine {
    /// Check if the engine has a model loaded
    pub async fn is_model_loaded(&self) -> bool {
        match self {
            Self::Whisper(engine) => engine.is_model_loaded().await,
            Self::Provider(provider) => provider.is_model_loaded().await,
        }
    }

    /// Get the current model name
    pub async fn get_current_model(&self) -> Option<String> {
        match self {
            Self::Whisper(engine) => engine.get_current_model().await,
            Self::Provider(provider) => provider.get_current_model().await,
        }
    }

    /// Get the provider name for logging
    pub fn provider_name(&self) -> &str {
        match self {
            Self::Whisper(_) => "Whisper (direct)",
            Self::Provider(provider) => provider.provider_name(),
        }
    }
}

// ============================================================================
// PROVIDER DISPATCH
// ============================================================================

#[derive(Debug)]
pub(crate) enum EngineKind {
    Whisper,
    Provider,
}

/// provider 字符串 → 引擎类型。未知 provider 一律报错，不再静默回退 Whisper。
pub(crate) fn engine_kind_for_provider(provider: &str) -> Result<EngineKind, String> {
    match provider {
        "localWhisper" => Ok(EngineKind::Whisper),
        "senseVoice" => Ok(EngineKind::Provider),
        other => Err(format!(
            "不支持的转写提供方「{other}」，请选择「本地 SenseVoice」或「本地 Whisper」。"
        )),
    }
}

fn sensevoice_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败：{e}"))?;
    Ok(crate::audio::transcription::sensevoice::model::sensevoice_model_dir(&data))
}

pub(crate) fn validate_sense_voice_ready(dir: &Path) -> Result<(), String> {
    let status = crate::audio::transcription::sensevoice::model::inspect_model_dir(dir);
    if status.state == "available" {
        Ok(())
    } else {
        Err("尚未下载 SenseVoice 模型，请在「设置 → 转写」中点击「下载模型」。".to_string())
    }
}

// ============================================================================
// MODEL VALIDATION AND INITIALIZATION
// ============================================================================

/// Validate that transcription models (Whisper) are ready before starting recording
pub async fn validate_transcription_model_ready<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    // Check transcript configuration to determine which engine to validate
    let config = match crate::api::api::api_get_transcript_config(
        app.clone(),
        app.clone().state(),
        None,
    )
    .await
    {
        Ok(Some(config)) => {
            info!(
                "📝 Found transcript config - provider: {}, model: {}",
                config.provider, config.model
            );
            config
        }
        Ok(None) => {
            info!("📝 No transcript config found, defaulting to localWhisper");
            crate::api::api::TranscriptConfig {
                provider: "localWhisper".to_string(),
                model: crate::config::DEFAULT_WHISPER_MODEL.to_string(),
                api_key: None,
            }
        }
        Err(e) => {
            warn!("⚠️ Failed to get transcript config: {}, defaulting to localWhisper", e);
            crate::api::api::TranscriptConfig {
                provider: "localWhisper".to_string(),
                model: crate::config::DEFAULT_WHISPER_MODEL.to_string(),
                api_key: None,
            }
        }
    };

    // Validate based on provider
    match config.provider.as_str() {
        "localWhisper" => {
            info!("🔍 Validating Whisper model...");
            // Ensure whisper engine is initialized first
            if let Err(init_error) = crate::whisper_engine::commands::whisper_init().await {
                warn!("❌ Failed to initialize Whisper engine: {}", init_error);
                return Err(format!(
                    "语音识别初始化失败：{}",
                    init_error
                ));
            }

            // Call the whisper validation command with config support
            match crate::whisper_engine::commands::whisper_validate_model_ready_with_config(app).await {
                Ok(model_name) => {
                    info!("✅ Whisper model validation successful: {} is ready", model_name);
                    Ok(())
                }
                Err(e) => {
                    warn!("❌ Whisper model validation failed: {}", e);
                    Err(e)
                }
            }
        }
        "senseVoice" => {
            info!("🔍 Validating SenseVoice model...");
            validate_sense_voice_ready(&sensevoice_dir(app)?)
        }
        other => {
            warn!("❌ Unsupported transcription provider for local recording: {}", other);
            Err(engine_kind_for_provider(other).unwrap_err())
        }
    }
}

/// Get or initialize the appropriate transcription engine based on provider configuration
pub async fn get_or_init_transcription_engine<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<TranscriptionEngine, String> {
    // Get provider configuration from API
    let config = match crate::api::api::api_get_transcript_config(
        app.clone(),
        app.clone().state(),
        None,
    )
    .await
    {
        Ok(Some(config)) => {
            info!(
                "📝 Transcript config - provider: {}, model: {}",
                config.provider, config.model
            );
            config
        }
        Ok(None) => {
            info!("📝 No transcript config found, defaulting to localWhisper");
            crate::api::api::TranscriptConfig {
                provider: "localWhisper".to_string(),
                model: crate::config::DEFAULT_WHISPER_MODEL.to_string(),
                api_key: None,
            }
        }
        Err(e) => {
            warn!("⚠️ Failed to get transcript config: {}, defaulting to localWhisper", e);
            crate::api::api::TranscriptConfig {
                provider: "localWhisper".to_string(),
                model: crate::config::DEFAULT_WHISPER_MODEL.to_string(),
                api_key: None,
            }
        }
    };

    // Initialize the appropriate engine based on provider
    match config.provider.as_str() {
        "localWhisper" => {
            info!("🎤 Initializing Whisper transcription engine");
            let whisper_engine = get_or_init_whisper(app).await?;
            Ok(TranscriptionEngine::Whisper(whisper_engine))
        }
        "senseVoice" => {
            info!("🎤 Initializing SenseVoice transcription engine");
            let provider = Arc::new(SenseVoiceProvider::new(sensevoice_dir(app)?));
            // worker 在 transcribe 前会检查 is_model_loaded()；懒加载 provider 必须在这里预加载，
            // 否则每段音频都会被判 ModelNotLoaded，整场得到空逐字稿。
            provider
                .ensure_loaded()
                .await
                .map_err(|e| format!("SenseVoice 模型加载失败：{e}"))?;
            Ok(TranscriptionEngine::Provider(provider))
        }
        other => {
            // 未知 provider 必须报错：静默回退 Whisper 会造成「校验报错但引擎仍用 Whisper」的分裂。
            warn!("Provider '{}' is not supported", other);
            Err(engine_kind_for_provider(other).unwrap_err())
        }
    }
}

/// Get or initialize transcription engine using API configuration
/// Returns Whisper engine if provider is localWhisper, otherwise returns error for non-Whisper providers
pub async fn get_or_init_whisper<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Arc<crate::whisper_engine::WhisperEngine>, String> {
    // Check if engine already exists and has a model loaded
    let existing_engine = {
        let engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE
            .lock()
            .unwrap();
        engine_guard.as_ref().cloned()
    };

    if let Some(engine) = existing_engine {
        // Check if a model is already loaded
        if engine.is_model_loaded().await {
            let current_model = engine
                .get_current_model()
                .await
                .unwrap_or_else(|| "unknown".to_string());

            // NEW: Check if loaded model matches saved config
            let configured_model = match crate::api::api::api_get_transcript_config(
                app.clone(),
                app.clone().state(),
                None,
            )
            .await
            {
                Ok(Some(config)) => {
                    info!(
                        "📝 Saved transcript config - provider: {}, model: {}",
                        config.provider, config.model
                    );
                    if config.provider == "localWhisper" && !config.model.is_empty() {
                        Some(config.model)
                    } else {
                        None
                    }
                }
                Ok(None) => {
                    info!("📝 No transcript config found in database");
                    None
                }
                Err(e) => {
                    warn!("⚠️ Failed to get transcript config: {}", e);
                    None
                }
            };

            // If loaded model matches config, reuse it
            if let Some(ref expected_model) = configured_model {
                if current_model == *expected_model {
                    info!(
                        "✅ Loaded model '{}' matches saved config, reusing",
                        current_model
                    );
                    return Ok(engine);
                } else {
                    info!(
                        "🔄 Loaded model '{}' doesn't match saved config '{}', reloading correct model...",
                        current_model, expected_model
                    );
                    // Unload the incorrect model
                    engine.unload_model().await;
                    info!("📉 Unloaded incorrect model '{}'", current_model);
                    // Continue to model loading logic below
                }
            } else {
                // No specific config saved, accept currently loaded model
                info!(
                    "✅ No specific model configured, using currently loaded model: '{}'",
                    current_model
                );
                return Ok(engine);
            }
        } else {
            info!("🔄 Whisper engine exists but no model loaded, will load model from config");
        }
    }

    // Initialize new engine if needed
    info!("Initializing Whisper engine");

    // First ensure the engine is initialized
    if let Err(e) = crate::whisper_engine::commands::whisper_init().await {
        return Err(format!("Whisper 引擎初始化失败：{}", e));
    }

    // Get the engine reference
    let engine = {
        let engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE
            .lock()
            .unwrap();
        engine_guard
            .as_ref()
            .cloned()
            .ok_or("无法获取已初始化的引擎")?
    };

    // Get model configuration from API
    let model_to_load =
        match crate::api::api::api_get_transcript_config(app.clone(), app.clone().state(), None)
            .await
        {
            Ok(Some(config)) => {
                info!(
                    "Got transcript config from API - provider: {}, model: {}",
                    config.provider, config.model
                );
                if config.provider == "localWhisper" {
                    info!("Using model from API config: {}", config.model);
                    config.model
                } else {
                    // Non-Whisper provider - this function shouldn't be called
                    return Err(format!(
                        "无法初始化 Whisper 引擎：配置使用「{}」提供方。这是转写任务初始化中的内部错误。",
                        config.provider
                    ));
                }
            }
            Ok(None) => {
                info!("No transcript config found in API, falling back to 'small'");
                "small".to_string()
            }
            Err(e) => {
                warn!(
                    "Failed to get transcript config from API: {}, falling back to 'small'",
                    e
                );
                "small".to_string()
            }
        };

    info!("Selected model to load: {}", model_to_load);

    // Discover available models to check if the desired model is downloaded
    let models = engine
        .discover_models()
        .await
        .map_err(|e| format!("无法发现模型：{}", e))?;

    info!("Discovered {} models", models.len());
    for model in &models {
        info!(
            "Model: {} - Status: {:?} - Path: {}",
            model.name,
            model.status,
            model.path.display()
        );
    }

    // Check if the desired model is available
    let model_info = models.iter().find(|model| model.name == model_to_load);

    if model_info.is_none() {
        info!(
            "Model '{}' not found in discovered models. Available models: {:?}",
            model_to_load,
            models.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }

    match model_info {
        Some(model) => {
            match model.status {
                crate::whisper_engine::ModelStatus::Available => {
                    info!("Loading model: {}", model_to_load);
                    engine
                        .load_model(&model_to_load)
                        .await
                        .map_err(|e| format!("模型「{}」加载失败：{}", model_to_load, e))?;
                    info!("✅ Model '{}' loaded successfully", model_to_load);
                }
                crate::whisper_engine::ModelStatus::Missing => {
                    return Err(format!(
                        "模型「{}」尚未下载，请先在设置中下载。",
                        model_to_load
                    ));
                }
                crate::whisper_engine::ModelStatus::Downloading { progress } => {
                    return Err(format!("模型「{}」正在下载（{}%），请等待下载完成。", model_to_load, progress));
                }
                crate::whisper_engine::ModelStatus::Error(ref err) => {
                    return Err(format!("模型「{}」出现错误：{}。请检查该模型或重新下载。", model_to_load, err));
                }
                crate::whisper_engine::ModelStatus::Corrupted { .. } => {
                    return Err(format!("模型「{}」已损坏，请在设置中删除后重新下载。", model_to_load));
                }
            }
        }
        None => {
            // Check if we have any available models and try to load the first one
            let available_models: Vec<_> = models
                .iter()
                .filter(|m| matches!(m.status, crate::whisper_engine::ModelStatus::Available))
                .collect();

            if let Some(fallback_model) = available_models.first() {
                warn!(
                    "Model '{}' not found, falling back to available model: '{}'",
                    model_to_load, fallback_model.name
                );
                engine.load_model(&fallback_model.name).await.map_err(|e| {
                    format!(
                        "备用模型「{}」加载失败：{}",
                        fallback_model.name, e
                    )
                })?;
                info!(
                    "✅ Fallback model '{}' loaded successfully",
                    fallback_model.name
                );
            } else {
                return Err(format!("模型「{}」不受支持，且没有其他可用模型。请在设置中下载模型。", model_to_load));
            }
        }
    }

    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sense_voice_without_model_returns_actionable_error() {
        let tmp = tempfile::tempdir().unwrap();
        let err = validate_sense_voice_ready(tmp.path()).unwrap_err();
        assert!(
            err.contains("尚未下载 SenseVoice 模型"),
            "错误必须告诉用户去哪下载，实际：{err}"
        );
    }

    #[test]
    fn unknown_provider_is_rejected_not_silently_fallback() {
        let err = engine_kind_for_provider("someUnknownProvider").unwrap_err();
        assert!(err.contains("不支持的转写提供方"), "实际：{err}");
    }

    #[test]
    fn known_providers_map_to_expected_engine_kind() {
        assert!(matches!(
            engine_kind_for_provider("senseVoice"),
            Ok(EngineKind::Provider)
        ));
        assert!(matches!(
            engine_kind_for_provider("localWhisper"),
            Ok(EngineKind::Whisper)
        ));
    }
}
