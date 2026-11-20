//! 紧急程度推断 — 移植自 `meeting_transcribe/assistant/tools/urgency.py`。
//! `HIGH_KEYWORDS` / `MEDIUM_KEYWORDS` 在 Python 侧是死代码，不移植。

use chrono::NaiveDate;

const URGENT_KEYWORDS: [&str; 6] = ["紧急", "立即", "加急", "马上", "今日", "今天截止"];

/// 关键词优先，其次按截止日期距 `today` 的天数分桶；无有效日期默认「中」。
///
/// `today` 由调用方注入，函数内不读系统时间。
pub fn determine_urgency(
    deadline: Option<&str>,
    task_name: &str,
    description: &str,
    today: NaiveDate,
) -> &'static str {
    let text = format!("{task_name} {description}");
    if URGENT_KEYWORDS.iter().any(|kw| text.contains(kw)) {
        return "紧急";
    }

    if let Some(raw) = deadline {
        if let Ok(d) = NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d") {
            return match (d - today).num_days() {
                ..=3 => "紧急", // 已过期 / 今天 / 3 天内
                ..=7 => "高",   // 一周内
                ..=30 => "中",  // 两周内 / 一个月内
                _ => "低",
            };
        }
    }

    "中"
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 4).unwrap()
    }

    #[test]
    fn urgent_keywords_win() {
        assert_eq!(
            determine_urgency(Some("2026-12-31"), "紧急处理", "", today()),
            "紧急"
        );
        assert_eq!(
            determine_urgency(None, "今天截止的报价", "", today()),
            "紧急"
        );
    }

    #[test]
    fn deadline_buckets() {
        assert_eq!(determine_urgency(Some("2026-08-04"), "a", "", today()), "紧急");
        assert_eq!(determine_urgency(Some("2026-08-07"), "a", "", today()), "紧急");
        assert_eq!(determine_urgency(Some("2026-08-10"), "a", "", today()), "高");
        assert_eq!(determine_urgency(Some("2026-08-18"), "a", "", today()), "中");
        assert_eq!(determine_urgency(Some("2026-09-30"), "a", "", today()), "低");
    }

    #[test]
    fn missing_or_bad_deadline_is_medium() {
        assert_eq!(determine_urgency(None, "a", "", today()), "中");
        assert_eq!(determine_urgency(Some("尽快"), "a", "", today()), "中");
    }
}
