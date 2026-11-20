//! 双协议请求头与请求体构造。

use super::Protocol;
use serde_json::{json, Value};

/// 按协议生成鉴权头：OpenAI 只带 `Authorization`，Anthropic 只带 `x-api-key` + `anthropic-version`。
pub fn auth_headers(protocol: Protocol, api_key: &str) -> Vec<(&'static str, String)> {
    match protocol {
        Protocol::OpenAi => vec![("Authorization", format!("Bearer {api_key}"))],
        Protocol::Anthropic => vec![
            ("x-api-key", api_key.to_string()),
            ("anthropic-version", "2023-06-01".to_string()),
        ],
    }
}

/// 两种协议都给 8192：推理型模型（如 PakClaw-Max）的 reasoning 内容会先吃掉预算，
/// 2048 时正文可能被截成空串（Phase 3.5 装机验收实测）。
pub fn default_max_tokens(_protocol: Protocol) -> i32 {
    8192
}

pub fn chat_body(protocol: Protocol, model: &str, system: &str, user: &str, max_tokens: i32) -> Value {
    match protocol {
        Protocol::OpenAi => json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ]
        }),
        Protocol::Anthropic => json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": [{"role": "user", "content": user}]
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summary::Protocol;

    #[test]
    fn openai_uses_bearer_and_no_anthropic_headers() {
        let h = auth_headers(Protocol::OpenAi, "sk-x");
        assert!(h.contains(&("Authorization", "Bearer sk-x".to_string())));
        assert!(!h.iter().any(|(k, _)| k.eq_ignore_ascii_case("x-api-key")));
        assert!(!h.iter().any(|(k, _)| k.eq_ignore_ascii_case("anthropic-version")));
    }

    #[test]
    fn anthropic_uses_x_api_key_and_version() {
        let h = auth_headers(Protocol::Anthropic, "sk-x");
        assert!(h.contains(&("x-api-key", "sk-x".to_string())));
        assert!(h.contains(&("anthropic-version", "2023-06-01".to_string())));
        assert!(!h.iter().any(|(k, _)| k.eq_ignore_ascii_case("authorization")));
    }

    #[test]
    fn anthropic_body_has_system_top_level_and_messages() {
        let body = chat_body(Protocol::Anthropic, "m", "sys", "hi", 8192);
        assert_eq!(body["system"], "sys");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(body["max_tokens"], 8192);
    }

    #[test]
    fn openai_body_puts_system_in_messages() {
        let body = chat_body(Protocol::OpenAi, "m", "sys", "hi", 2048);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["max_tokens"], 2048);
    }

    #[test]
    fn default_max_tokens_leaves_room_for_reasoning_models() {
        // 推理型模型先输出 reasoning 再输出正文，2048 会把正文挤成空串
        assert_eq!(default_max_tokens(Protocol::Anthropic), 8192);
        assert_eq!(default_max_tokens(Protocol::OpenAi), 8192);
    }
}
