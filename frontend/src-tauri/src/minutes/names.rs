//! 参会人员抽取 — 移植自 `meeting_transcribe/assistant/tools/orchestrator.py` 的
//! `_step4_normalize_attendees`。

use regex::Regex;
use std::sync::OnceLock;

static ATTENDEES: OnceLock<Regex> = OnceLock::new();

/// 从原文抽取「参会人员：…」一行，按名单分成 (known, unknown)。
///
/// 返回 `(匹配到名单的人, 名单外的人)`，顺序与原文一致。
pub fn extract_attendees(text: &str, members: &[String]) -> (Vec<String>, Vec<String>) {
    let re = ATTENDEES.get_or_init(|| {
        Regex::new(r"(?:参会|与会|出席)[人人员]*[：:]\s*([^。；;\n]+)").expect("static regex")
    });

    let mut known = Vec::new();
    let mut unknown = Vec::new();

    if let Some(caps) = re.captures(text) {
        let names = caps[1]
            .split(|ch: char| matches!(ch, ',' | '，' | '、') || ch.is_whitespace())
            .filter(|name| !name.is_empty());
        for name in names {
            if members.iter().any(|m| m == name) {
                known.push(name.to_string());
            } else {
                unknown.push(name.to_string());
            }
        }
    }

    (known, unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn members() -> Vec<String> {
        vec!["许日政".into(), "杨朋".into()]
    }

    #[test]
    fn extracts_known_and_unknown_names() {
        let (known, unknown) = extract_attendees("参会人员：许日政，杨朋，张三", &members());
        assert_eq!(known, vec!["许日政", "杨朋"]);
        assert_eq!(unknown, vec!["张三"]);
    }

    #[test]
    fn handles_colon_variants_and_spaces() {
        let (known, _) = extract_attendees("与会人员: 许日政 、 杨朋", &members());
        assert_eq!(known, vec!["许日政", "杨朋"]);
    }

    #[test]
    fn no_attendee_line_yields_empty() {
        let (known, unknown) = extract_attendees("今天讨论路灯方案。", &members());
        assert!(known.is_empty() && unknown.is_empty());
    }
}
