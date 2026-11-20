//! 端点归一化：用户可填 base URL、`/v1` 或完整端点，统一拼成对话/模型地址。

use super::Protocol;

/// 把用户输入（base URL 或完整 URL）归一化成对话端点。
pub fn chat_url(endpoint: &str, protocol: Protocol) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    let (suffix, other) = match protocol {
        Protocol::OpenAi => ("/chat/completions", "/messages"),
        Protocol::Anthropic => ("/messages", "/chat/completions"),
    };
    if trimmed.ends_with(suffix) {
        return trimmed.to_string();
    }
    // 切协议后残留的对方协议完整端点先剥掉，避免拼成 /v1/messages/v1/chat/completions
    let trimmed = trimmed.strip_suffix(other).unwrap_or(trimmed);
    let base = if trimmed.ends_with("/v1") { trimmed.to_string() } else { format!("{trimmed}/v1") };
    format!("{base}{suffix}")
}

pub fn models_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with("/models") {
        return trimmed.to_string();
    }
    let trimmed = trimmed
        .strip_suffix("/chat/completions")
        .or_else(|| trimmed.strip_suffix("/messages"))
        .unwrap_or(trimmed);
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
        // 切协议后残留的对方协议完整 URL 不能被二次拼接
        assert_eq!(
            chat_url("http://gw:4000/v1/messages", Protocol::OpenAi),
            "http://gw:4000/v1/chat/completions"
        );
        assert_eq!(
            chat_url("http://gw:4000/v1/chat/completions", Protocol::Anthropic),
            "http://gw:4000/v1/messages"
        );
        assert_eq!(models_url("http://gw:4000/v1/chat/completions"), "http://gw:4000/v1/models");
        assert_eq!(models_url("http://gw:4000/v1/messages"), "http://gw:4000/v1/models");
    }

    #[test]
    fn anthropic_appends_messages_not_chat_completions() {
        assert_eq!(chat_url("http://gw:4000", Protocol::Anthropic), "http://gw:4000/v1/messages");
        assert_eq!(chat_url("http://gw:4000/v1", Protocol::Anthropic), "http://gw:4000/v1/messages");
    }

    #[test]
    fn trailing_slash_and_whitespace_are_tolerated() {
        assert_eq!(chat_url("  http://gw:4000/v1/  ", Protocol::OpenAi), "http://gw:4000/v1/chat/completions");
        // 空串防御：不 panic，也不拼出无 host 的 /v1/... 地址
        assert_eq!(chat_url("", Protocol::OpenAi), "");
        assert_eq!(models_url(""), "");
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
        // 序列化契约：新配置写回时必须带上 protocol
        assert!(serde_json::to_string(&cfg).unwrap().contains("\"protocol\":\"openai\""));
    }
}
