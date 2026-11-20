//! 端点归一化：用户可填 base URL、`/v1` 或完整端点，统一拼成对话/模型地址。

use super::Protocol;

/// 把用户输入（base URL 或完整 URL）归一化成对话端点。
pub fn chat_url(endpoint: &str, protocol: Protocol) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let suffix = match protocol {
        Protocol::OpenAi => "/chat/completions",
        Protocol::Anthropic => "/messages",
    };
    if trimmed.ends_with(suffix) {
        return trimmed.to_string();
    }
    let base = if trimmed.ends_with("/v1") { trimmed.to_string() } else { format!("{trimmed}/v1") };
    format!("{base}{suffix}")
}

pub fn models_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.ends_with("/models") {
        return trimmed.to_string();
    }
    let base = if trimmed.ends_with("/v1") { trimmed.to_string() } else { format!("{trimmed}/v1") };
    format!("{base}/models")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summary::Protocol;

    #[test]
    fn base_url_gets_chat_completions_for_openai() {
        assert_eq!(chat_url("http://gw:4000", Protocol::OpenAi), "http://gw:4000/v1/chat/completions");
        assert_eq!(chat_url("http://gw:4000/v1", Protocol::OpenAi), "http://gw:4000/v1/chat/completions");
    }

    #[test]
    fn full_url_is_left_alone() {
        assert_eq!(
            chat_url("http://gw:4000/v1/chat/completions", Protocol::OpenAi),
            "http://gw:4000/v1/chat/completions"
        );
        assert_eq!(
            chat_url("http://gw:4000/v1/messages", Protocol::Anthropic),
            "http://gw:4000/v1/messages"
        );
    }

    #[test]
    fn anthropic_appends_messages_not_chat_completions() {
        assert_eq!(chat_url("http://gw:4000", Protocol::Anthropic), "http://gw:4000/v1/messages");
        assert_eq!(chat_url("http://gw:4000/v1", Protocol::Anthropic), "http://gw:4000/v1/messages");
    }

    #[test]
    fn trailing_slash_and_whitespace_are_tolerated() {
        assert_eq!(chat_url("  http://gw:4000/v1/  ", Protocol::OpenAi), "http://gw:4000/v1/chat/completions");
    }

    #[test]
    fn models_url_always_uses_v1_models() {
        assert_eq!(models_url("http://gw:4000"), "http://gw:4000/v1/models");
        assert_eq!(models_url("http://gw:4000/v1"), "http://gw:4000/v1/models");
    }

    #[test]
    fn old_config_without_protocol_defaults_to_openai() {
        let json = r#"{"endpoint":"http://gw:4000","model":"m","apiKey":null,"maxTokens":null,"temperature":null,"topP":null}"#;
        let cfg: crate::summary::CustomOpenAIConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(cfg.protocol, Protocol::OpenAi));
    }
}
