//! 自然语言日期解析 — 移植自 `meeting_transcribe/assistant/tools/date_parser.py`。

use chrono::{Datelike, Duration, NaiveDate};
use regex::Regex;
use std::sync::OnceLock;

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("static date regex"))
}

static ISO: OnceLock<Regex> = OnceLock::new();
static WEEK: OnceLock<Regex> = OnceLock::new();
static MONTH_DAY: OnceLock<Regex> = OnceLock::new();
static SHORT: OnceLock<Regex> = OnceLock::new();
static FULL: OnceLock<Regex> = OnceLock::new();

/// Python 侧按此顺序逐个 `endswith` 判断，命中第一个即停。
const SUFFIXES: [&str; 13] = [
    "前完成", "前", "后完成", "后", "内完成", "内", "以前", "以前完成", "前截止", "截止", "完成", "之前",
    "之后",
];

/// 将自然语言日期解析为具体日期；无法解析返回 `None`。
///
/// `today` 由调用方注入，函数内不读系统时间。
pub fn parse_date(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    let text = input.trim();

    // 已是 YYYY-MM-DD：Python 原样返回字符串，这里必须真解析，非法日期返回 None
    if regex(&ISO, r"^\d{4}-\d{2}-\d{2}$").is_match(text) {
        return NaiveDate::parse_from_str(text, "%Y-%m-%d").ok();
    }

    let mut clean = text;
    for suffix in SUFFIXES {
        if let Some(stripped) = clean.strip_suffix(suffix) {
            clean = stripped;
            break;
        }
    }

    // Python 这里判断的是原始 text，不是剥尾缀后的 clean
    let offset = match text {
        "今天" | "今日" => Some(0),
        "明天" | "明日" => Some(1),
        "后天" | "后日" => Some(2),
        "昨天" | "昨日" => Some(-1),
        _ => None,
    };
    if let Some(days) = offset {
        return Some(today + Duration::days(days));
    }

    if let Some(c) = regex(&WEEK, r"^(本|下|上)(周|星期)([一二三四五六日天])$").captures(clean) {
        let target = match &c[3] {
            "一" => 0,
            "二" => 1,
            "三" => 2,
            "四" => 3,
            "五" => 4,
            "六" => 5,
            _ => 6,
        };
        let current = today.weekday().num_days_from_monday() as i64;
        let days = match &c[1] {
            "本" => target - current,
            "下" => target - current + 7,
            _ => target - current - 7,
        };
        return Some(today + Duration::days(days));
    }

    if let Some(c) = regex(&MONTH_DAY, r"^(\d{1,2})月(\d{1,2})[日号]?$").captures(clean) {
        return roll_over(c[1].parse().ok()?, c[2].parse().ok()?, today);
    }

    if let Some(c) = regex(&SHORT, r"^(\d{1,2})[-./](\d{1,2})$").captures(clean) {
        return roll_over(c[1].parse().ok()?, c[2].parse().ok()?, today);
    }

    if let Some(c) = regex(&FULL, r"^(\d{4})[-./](\d{1,2})[-./](\d{1,2})$").captures(clean) {
        return NaiveDate::from_ymd_opt(c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?);
    }

    None
}

/// 只有月日时，若已过则推定到明年。
fn roll_over(month: u32, day: u32, today: NaiveDate) -> Option<NaiveDate> {
    let d = NaiveDate::from_ymd_opt(today.year(), month, day)?;
    if d < today {
        NaiveDate::from_ymd_opt(today.year() + 1, month, day)
    } else {
        Some(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 4).unwrap()
    } // 周二

    #[test]
    fn iso_date_passes_through() {
        assert_eq!(
            parse_date("2026-08-15", today()),
            Some(NaiveDate::from_ymd_opt(2026, 8, 15).unwrap())
        );
    }

    #[test]
    fn relative_days_resolve_against_reference() {
        assert_eq!(parse_date("今天", today()), Some(today()));
        assert_eq!(
            parse_date("明天", today()),
            Some(NaiveDate::from_ymd_opt(2026, 8, 5).unwrap())
        );
        assert_eq!(
            parse_date("后天", today()),
            Some(NaiveDate::from_ymd_opt(2026, 8, 6).unwrap())
        );
    }

    #[test]
    fn weekdays_resolve_to_next_occurrence() {
        // 2026-08-04 是周二；本周五 = 08-07，下周三 = 08-12
        assert_eq!(
            parse_date("本周五", today()),
            Some(NaiveDate::from_ymd_opt(2026, 8, 7).unwrap())
        );
        assert_eq!(
            parse_date("下周三", today()),
            Some(NaiveDate::from_ymd_opt(2026, 8, 12).unwrap())
        );
        // 有意偏离 Python：它的正则写成了字符类 [星期]，匹配不到「星期三」；
        // Rust 用 (周|星期) 正确接受，Python 版对这类输入返回空。
        assert_eq!(
            parse_date("本星期三", today()),
            Some(NaiveDate::from_ymd_opt(2026, 8, 5).unwrap())
        );
    }

    #[test]
    fn month_day_rolls_to_next_year_when_past() {
        assert_eq!(
            parse_date("8月15日", today()),
            Some(NaiveDate::from_ymd_opt(2026, 8, 15).unwrap())
        );
        assert_eq!(
            parse_date("3月1日", today()),
            Some(NaiveDate::from_ymd_opt(2027, 3, 1).unwrap())
        );
    }

    #[test]
    fn strips_trailing_suffixes() {
        assert_eq!(
            parse_date("2026-08-15前完成", today()),
            Some(NaiveDate::from_ymd_opt(2026, 8, 15).unwrap())
        );
    }

    #[test]
    fn garbage_returns_none() {
        assert_eq!(parse_date("尽快", today()), None);
        assert_eq!(parse_date("", today()), None);
    }
}
