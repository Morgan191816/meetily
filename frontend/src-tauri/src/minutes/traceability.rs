//! 溯源核对 — 移植自 `meeting_transcribe/assistant/tools/traceability.py`。
//!
//! 阈值表、trigger 词表、清洗规则逐条照抄 Python。三处 Python 自身的行为需要留意：
//! 1. 子串搜索是「长度 4 起升序试探、首个命中即停」，所以每个起点最多只产出 4 字匹配，
//!    `longest_match_len` 恒为 4 或 0 —— ≥6 / ≥8 两档在生产路径上不可达，仍按规格保留。
//! 2. 无子串时 `sources` 必为空，decision 的「ratio≥0.4 且 trigger 且 sources」档同样不可达。
//! 3. `_extract_keywords` 用 `set` 迭代停用词，切分顺序随 PYTHONHASHSEED 变化；这里按源文件
//!    书写顺序切分，是其中一个合法结果（同一输入可能得到不同的关键词列表，进而影响 ratio）。

use std::collections::HashSet;

// ── trigger 词表（逐字照抄 traceability.py:30-56）──
const DECISION_TRIGGERS: &[&str] = &[
    "确认", "决定", "同意", "通过", "批准", "确定", "定了", "就这么办", "可以", "行", "没问题",
    "就这样", "结论", "决议", "采纳",
];

const TASK_TRIGGERS: &[&str] = &[
    "负责", "跟进", "安排", "处理", "完成", "落实", "做", "写", "评估", "确认", "对接", "协调", "你",
    "你们", "我", "我们", // 人称代词暗示任务分配
    "拉", "拉个群", "拉群", "建群", "建个群", "整理", "列", "列个", "列一下", "提供", "发", "发到",
    "发一下", "安排", "叫", "叫上", "约", "给", "给到", "给咱们", "做", "做一下", "处理", "处理一下",
    "准备", "准备好", "准备一下",
];

const RISK_TRIGGERS: &[&str] = &[
    "但是", "问题", "风险", "担心", "不确定", "可能不行", "难点", "困难", "挑战", "隐患", "注意",
    "成本高", "来不及", "做不了",
];

/// 关键词切分用停用词（仅用于切分），顺序照 Python 源文件书写顺序。
const STOPWORDS: &[&str] = &[
    "的", "了", "在", "是", "和", "就", "不", "也", "都", "要", "去", "会", "这", "那", "对", "把",
    "被", "让", "给", "跟", "与", "及", "等", "或", "但", "所以", "因为", "如果", "虽然", "而且",
    "但是", "可以", "可能", "应该", "需要", "能够", "已经", "没有", "还是", "或者", "不是", "就是",
    "只是", "不过", "并且", "比较", "非常", "特别", "目前", "现在", "之前", "之后", "同时", "其中",
    "进行", "通过", "使用", "提出", "发现", "认为", "作为", "成为", "有些", "很多", "一些", "每个",
    "所有", "任何", "什么", "怎么", "怎样", "如何", "为什么", "多少", "我们", "他们", "你们", "咱们",
    "大家",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Verified,
    Inferred,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// 声明类型；对应 Python 的 `claim_type`（其余取值在 Python 里走 risk 分支）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    Decision,
    Task,
    Risk,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimResult {
    pub claim: String,
    pub verdict: Verdict,
    pub confidence: Confidence,
    pub sources: Vec<String>,
}

/// 校验单条声明能否从原文找到依据。
pub fn verify_claim(claim: &str, source: &str, kind: ClaimKind) -> ClaimResult {
    let substrings = longest_common_substrings(claim, source, 4);
    let longest_match_len = substrings.first().map_or(0, |m| m.length);

    // Python 用清洗后文本的下标去切原文，这里照做以保持出处片段一致。
    let source_chars: Vec<char> = source.chars().collect();
    let sources: Vec<String> = substrings
        .iter()
        .take(3)
        .map(|m| {
            let start = m.start.saturating_sub(30);
            let end = (m.start + m.length + 30).min(source_chars.len());
            collapse_whitespace(&source_chars[start..end])
        })
        .collect();

    let claim_keywords = extract_keywords(claim);
    let triggers = triggers_for(kind);
    let source_lower = source.to_lowercase();
    let matched = claim_keywords
        .iter()
        .filter(|kw| source_lower.contains(kw.to_lowercase().as_str()))
        .count();
    let match_ratio = if claim_keywords.is_empty() {
        0.0
    } else {
        matched as f64 / claim_keywords.len() as f64
    };
    let has_trigger = triggers.iter().any(|t| source.contains(t));

    let (verdict, confidence) = judge(
        longest_match_len,
        match_ratio,
        has_trigger,
        kind,
        !sources.is_empty(),
    );

    ClaimResult {
        claim: claim.to_string(),
        verdict,
        confidence,
        sources,
    }
}

/// 批量校验，保持输入顺序。
pub fn verify_claims(claims: &[String], source: &str, kind: ClaimKind) -> Vec<ClaimResult> {
    claims
        .iter()
        .map(|claim| verify_claim(claim, source, kind))
        .collect()
}

fn triggers_for(kind: ClaimKind) -> &'static [&'static str] {
    match kind {
        ClaimKind::Decision => DECISION_TRIGGERS,
        ClaimKind::Task => TASK_TRIGGERS,
        ClaimKind::Risk => RISK_TRIGGERS,
    }
}

/// 判定表（traceability.py:162-222）。子串证据优先于关键词覆盖率。
fn judge(
    longest_match_len: usize,
    match_ratio: f64,
    has_trigger: bool,
    kind: ClaimKind,
    has_sources: bool,
) -> (Verdict, Confidence) {
    if longest_match_len >= 8 {
        (Verdict::Verified, Confidence::High)
    } else if longest_match_len >= 6 {
        (Verdict::Verified, Confidence::Medium)
    } else if longest_match_len >= 4 {
        if match_ratio >= 0.3 || has_trigger {
            (Verdict::Verified, Confidence::Medium)
        } else {
            (Verdict::Inferred, Confidence::Low)
        }
    } else {
        match kind {
            ClaimKind::Decision => {
                if match_ratio >= 0.4 && has_trigger && has_sources {
                    (Verdict::Verified, Confidence::Medium)
                } else if match_ratio >= 0.2 {
                    (Verdict::Inferred, Confidence::Low)
                } else {
                    (Verdict::Unsupported, Confidence::Low)
                }
            }
            ClaimKind::Task => {
                if match_ratio >= 0.3 && has_trigger {
                    (Verdict::Verified, Confidence::Medium)
                } else if match_ratio >= 0.15 {
                    (Verdict::Inferred, Confidence::Low)
                } else {
                    (Verdict::Unsupported, Confidence::Low)
                }
            }
            ClaimKind::Risk => {
                if match_ratio >= 0.3 {
                    (Verdict::Verified, Confidence::Medium)
                } else if match_ratio >= 0.15 {
                    (Verdict::Inferred, Confidence::Low)
                } else {
                    (Verdict::Unsupported, Confidence::Low)
                }
            }
        }
    }
}

struct SubMatch {
    start: usize,
    length: usize,
}

/// 在原文中找声明的公共子串（traceability.py:398-446）。
///
/// 逐字复刻 Python：每个起点从 4 字起**升序**试探，首个命中即停。因此若某长度的子串命中，
/// 其 4 字前缀必然也命中 —— 每个起点实际只会产出 4 字匹配，长度上限分支是死代码。
fn longest_common_substrings(claim: &str, original: &str, min_len: usize) -> Vec<SubMatch> {
    let claim_chars = clean_chars(claim);
    let orig_chars = clean_chars(original);
    let mut matches: Vec<SubMatch> = Vec::new();

    if claim_chars.len() >= min_len {
        for i in 0..=claim_chars.len() - min_len {
            let remaining = claim_chars.len() - i;
            // Python: range(min_len, min(remaining + 1, 20)) → 上限含 min(remaining, 19)
            for length in min_len..=remaining.min(19) {
                let needle = &claim_chars[i..i + length];
                if let Some(start) = find_first(&orig_chars, needle) {
                    matches.push(SubMatch { start, length });
                    break;
                }
            }
        }
    }

    matches.sort_by(|a, b| b.length.cmp(&a.length)); // 稳定排序，等长保持起点升序

    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut unique = Vec::new();
    for m in matches {
        if spans.iter().any(|&(s, e)| s <= m.start && m.start < e) {
            continue;
        }
        spans.push((m.start, m.start + m.length));
        unique.push(m);
    }
    unique.truncate(5);
    unique
}

/// 保留中文（U+4E00–U+9FFF）与 `\w`，其余字符去掉 —— 对应 `[^一-鿿\w]`。
fn clean_chars(text: &str) -> Vec<char> {
    text.chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c) || c.is_alphanumeric() || *c == '_')
        .collect()
}

fn find_first(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&p| &haystack[p..p + needle.len()] == needle)
}

/// 对应 Python 的 `.strip()` + `re.sub(r'\s+', ' ')`。
fn collapse_whitespace(chars: &[char]) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for &c in chars {
        if c.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(c);
        }
    }
    out
}

/// 对应 `re.split(r'[,.;:!?()，。；：！？（）\s]+')`；连续分隔符产生的空串在下面被丢弃。
fn is_segment_separator(c: char) -> bool {
    matches!(
        c,
        ',' | '.' | ';' | ':' | '!' | '?' | '(' | ')' | '，' | '。' | '；' | '：' | '！' | '？'
            | '（' | '）'
    ) || c.is_whitespace()
}

/// 声明关键词（traceability.py:319-395）：标点/停用词切分 → 2–6 字滑窗 → 去重截断 30。
fn extract_keywords(text: &str) -> Vec<String> {
    let mut cleaned = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '，' | '、' | '+' | '/' | '\\' => cleaned.push(','),
            '。' => cleaned.push('.'),
            '；' => cleaned.push(';'),
            '：' => cleaned.push(':'),
            '（' => cleaned.push('('),
            '）' => cleaned.push(')'),
            '"' | '「' | '」' | ' ' => {}
            _ => cleaned.push(c),
        }
    }

    let mut base: Vec<String> = Vec::new();
    for segment in cleaned.split(is_segment_separator) {
        if segment.chars().count() < 2 {
            continue;
        }
        let mut parts = vec![segment.to_string()];
        for &sw in STOPWORDS {
            let mut next = Vec::new();
            for part in &parts {
                for piece in part.split(sw) {
                    if !piece.is_empty() {
                        next.push(piece.to_string());
                    }
                }
            }
            parts = next;
        }
        base.extend(parts.into_iter().filter(|p| p.chars().count() >= 2));
    }

    let mut window_keywords: Vec<String> = Vec::new();
    for kw in &base {
        let n = kw.chars().count();
        if n < 4 {
            continue;
        }
        for length in (2..=6).rev() {
            let step = (length / 2).max(1);
            let limit = (n + 1).saturating_sub(length).max(1);
            let mut i = 0;
            while i < limit {
                if i + length <= n {
                    let chunk: String = kw.chars().skip(i).take(length).collect();
                    if chunk.chars().count() >= 2 {
                        window_keywords.push(chunk);
                    }
                }
                i += step;
            }
        }
    }
    base.extend(window_keywords);

    let mut seen: HashSet<String> = HashSet::new();
    let mut result = Vec::new();
    for w in base {
        if w.chars().count() >= 2 && seen.insert(w.clone()) {
            result.push(w);
            if result.len() == 30 {
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "会上确认华硕不落总灯已经结案了。冯亚的进度，龙哥这边帮忙跟进一下。预算还没批，可能来不及。";

    // ── brief 的 4 条行为测试 ──

    #[test]
    fn verbatim_claim_is_verified_medium() {
        // 与 brief 的「High」不同：Python 的子串循环升序试探、首个命中即停，
        // 10 字逐字引用也只得到 4 字匹配 → 4-5 档 → 中（见 longest_common_substrings）。
        let r = verify_claim("华硕不落总灯已经结案", SRC, ClaimKind::Decision);
        assert!(matches!(r.verdict, Verdict::Verified));
        assert!(matches!(r.confidence, Confidence::Medium));
        assert!(!r.sources.is_empty());
    }

    #[test]
    fn paraphrase_with_trigger_is_not_unsupported() {
        // brief 原句「跟进冯亚项目进度」在 Python 里是 unsupported（被「跟」切碎，覆盖率 0.12），
        // 这里改用 Python 同样判 verified 的口语化转述。
        let r = verify_claim("跟进一下冯亚的进度", SRC, ClaimKind::Task);
        assert!(matches!(r.verdict, Verdict::Verified));
    }

    #[test]
    fn unrelated_claim_is_unsupported() {
        let r = verify_claim("采购部同意追加三百万元预算", SRC, ClaimKind::Decision);
        assert!(matches!(r.verdict, Verdict::Unsupported));
    }

    #[test]
    fn sources_are_trimmed_around_match() {
        let r = verify_claim("华硕不落总灯已经结案", SRC, ClaimKind::Decision);
        assert!(r.sources[0].chars().count() <= 80, "出处片段应截断");
    }

    // ── 阈值表逐档 ──

    #[test]
    fn substring_8_plus_is_high() {
        assert_eq!(
            judge(8, 0.0, false, ClaimKind::Decision, false),
            (Verdict::Verified, Confidence::High)
        );
        assert_eq!(
            judge(19, 0.0, false, ClaimKind::Task, false),
            (Verdict::Verified, Confidence::High)
        );
    }

    #[test]
    fn substring_6_to_7_is_medium() {
        assert_eq!(
            judge(6, 0.0, false, ClaimKind::Risk, false),
            (Verdict::Verified, Confidence::Medium)
        );
        assert_eq!(
            judge(7, 0.0, false, ClaimKind::Risk, false),
            (Verdict::Verified, Confidence::Medium)
        );
    }

    #[test]
    fn substring_4_with_ratio_is_medium() {
        assert_eq!(
            judge(4, 0.3, false, ClaimKind::Decision, false),
            (Verdict::Verified, Confidence::Medium)
        );
    }

    #[test]
    fn substring_4_with_trigger_is_medium() {
        assert_eq!(
            judge(4, 0.0, true, ClaimKind::Decision, false),
            (Verdict::Verified, Confidence::Medium)
        );
    }

    #[test]
    fn substring_4_without_ratio_or_trigger_is_inferred() {
        assert_eq!(
            judge(4, 0.29, false, ClaimKind::Decision, false),
            (Verdict::Inferred, Confidence::Low)
        );
    }

    #[test]
    fn decision_without_substring_verified() {
        // Python 该档还要求 sources 非空，但无子串时 sources 必空 —— 死分支按规格保留。
        assert_eq!(
            judge(0, 0.4, true, ClaimKind::Decision, true),
            (Verdict::Verified, Confidence::Medium)
        );
    }

    #[test]
    fn decision_without_substring_inferred() {
        assert_eq!(
            judge(0, 0.4, true, ClaimKind::Decision, false),
            (Verdict::Inferred, Confidence::Low)
        );
        assert_eq!(
            judge(0, 0.2, false, ClaimKind::Decision, false),
            (Verdict::Inferred, Confidence::Low)
        );
    }

    #[test]
    fn decision_without_substring_unsupported() {
        assert_eq!(
            judge(0, 0.19, false, ClaimKind::Decision, false),
            (Verdict::Unsupported, Confidence::Low)
        );
    }

    #[test]
    fn task_without_substring_verified() {
        assert_eq!(
            judge(0, 0.3, true, ClaimKind::Task, false),
            (Verdict::Verified, Confidence::Medium)
        );
    }

    #[test]
    fn task_without_substring_inferred() {
        assert_eq!(
            judge(0, 0.3, false, ClaimKind::Task, false),
            (Verdict::Inferred, Confidence::Low)
        );
        assert_eq!(
            judge(0, 0.15, true, ClaimKind::Task, false),
            (Verdict::Inferred, Confidence::Low)
        );
    }

    #[test]
    fn task_without_substring_unsupported() {
        assert_eq!(
            judge(0, 0.14, true, ClaimKind::Task, false),
            (Verdict::Unsupported, Confidence::Low)
        );
    }

    #[test]
    fn risk_without_substring_verified() {
        assert_eq!(
            judge(0, 0.3, false, ClaimKind::Risk, false),
            (Verdict::Verified, Confidence::Medium)
        );
    }

    #[test]
    fn risk_without_substring_inferred() {
        assert_eq!(
            judge(0, 0.15, false, ClaimKind::Risk, false),
            (Verdict::Inferred, Confidence::Low)
        );
    }

    #[test]
    fn risk_without_substring_unsupported() {
        assert_eq!(
            judge(0, 0.14, true, ClaimKind::Risk, false),
            (Verdict::Unsupported, Confidence::Low)
        );
    }

    #[test]
    fn verify_claims_keeps_order_and_verdicts() {
        let claims = vec![
            "华硕不落总灯已经结案".to_string(),
            "采购部同意追加三百万元预算".to_string(),
        ];
        let results = verify_claims(&claims, SRC, ClaimKind::Decision);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].verdict, Verdict::Verified);
        assert_eq!(results[1].verdict, Verdict::Unsupported);
    }
}
