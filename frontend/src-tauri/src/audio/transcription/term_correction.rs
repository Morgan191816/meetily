// audio/transcription/term_correction.rs
//
// 转写后术语纠正：SenseVoice 不支持热词，用术语表把同音错字改回正确写法。

use std::path::PathBuf;
use std::sync::OnceLock;

static TERM_CORRECTOR: OnceLock<TermCorrector> = OnceLock::new();

pub struct TermCorrector {
    /// 别名（含正确写法自身）→ 正确写法，按字符数降序；同长时自身映射优先
    rules: Vec<(String, String)>,
}

impl TermCorrector {
    pub fn from_json(json: &str) -> Self {
        let mut pairs: Vec<(String, String)> = Vec::new();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
            collect(&value, &mut pairs);
        }
        // 长词优先；同一别名既是独立词条又是长词别名时保留自身映射，避免短词被改成长词
        pairs.sort_by(|a, b| {
            b.0.chars()
                .count()
                .cmp(&a.0.chars().count())
                .then_with(|| (b.0 == b.1).cmp(&(a.0 == a.1)))
        });
        Self { rules: pairs }
    }

    /// 单趟最长匹配扫描：命中规则整段输出正确写法，已正确的长词不会被内部短别名二次替换。
    pub fn correct(&self, text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < chars.len() {
            let mut hit = None;
            for (alias, correct) in &self.rules {
                let len = alias.chars().count();
                if len == 0 || i + len > chars.len() {
                    continue;
                }
                if chars[i..i + len].iter().copied().eq(alias.chars()) {
                    hit = Some((len, correct));
                    break; // rules 已按长度降序，首个命中即最长
                }
            }
            match hit {
                Some((len, correct)) => {
                    out.push_str(correct);
                    i += len;
                }
                None => {
                    out.push(chars[i]);
                    i += 1;
                }
            }
        }
        out
    }
}

fn collect(value: &serde_json::Value, out: &mut Vec<(String, String)>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(correct) = map.get("correct").and_then(|v| v.as_str()) {
                // 正确写法自身也入规则：防止「东软载波」里的「东软」被二次替换
                out.push((correct.to_string(), correct.to_string()));
                if let Some(aliases) = map.get("aliases").and_then(|v| v.as_array()) {
                    for alias in aliases.iter().filter_map(|v| v.as_str()) {
                        if alias != correct {
                            out.push((alias.to_string(), correct.to_string()));
                        }
                    }
                }
            }
            for (_, child) in map {
                collect(child, out);
            }
        }
        _ => {}
    }
}

/// 初始化全局术语表：`user`（用户覆盖）优先，其次 `bundled`（打包资源）；
/// 都读不到就用空表，不阻断转写。
pub fn init(user: Option<PathBuf>, bundled: Option<PathBuf>) {
    let loaded = user
        .and_then(|path| std::fs::read_to_string(&path).ok().map(|json| (path, json)))
        .or_else(|| {
            bundled.and_then(|path| std::fs::read_to_string(&path).ok().map(|json| (path, json)))
        });
    let (corrector, source) = match loaded {
        Some((path, json)) => (TermCorrector::from_json(&json), path.display().to_string()),
        None => (TermCorrector::from_json("{}"), "空表".to_string()),
    };
    log::info!(
        "术语表已加载：{} 条规则（来源：{}）",
        corrector.rules.len(),
        source
    );
    let _ = TERM_CORRECTOR.set(corrector);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_alias_with_correct_term() {
        let c = TermCorrector::from_json(
            r#"{"品牌":{"格兰斯贝":{"correct":"格兰斯贝","aliases":["格兰仕"]}}}"#,
        );
        assert_eq!(c.correct("我们和格兰仕合作"), "我们和格兰斯贝合作");
    }

    #[test]
    fn does_not_double_replace_longer_correct_term() {
        let c = TermCorrector::from_json(
            r#"{"品牌":{"东软载波":{"correct":"东软载波","aliases":["东软"]}}}"#,
        );
        assert_eq!(c.correct("东软载波的方案"), "东软载波的方案");
    }

    #[test]
    fn longest_alias_wins() {
        let c = TermCorrector::from_json(
            r#"{"a":{"东软载波":{"correct":"东软载波","aliases":["东软"]}},"b":{"东软":{"correct":"东软"}}}"#,
        );
        assert_eq!(c.correct("东软发布"), "东软发布");
    }

    #[test]
    fn skips_text_shorter_than_alias_without_panic() {
        let c = TermCorrector::from_json(
            r#"{"a":{"格兰斯贝":{"correct":"格兰斯贝","aliases":["格兰仕"]}}}"#,
        );
        assert_eq!(c.correct("格"), "格");
        assert_eq!(c.correct(""), "");
    }

    #[test]
    fn empty_or_invalid_json_yields_identity() {
        assert_eq!(TermCorrector::from_json("not json").correct("原文"), "原文");
        assert_eq!(TermCorrector::from_json("{}").correct("原文"), "原文");
    }
}
