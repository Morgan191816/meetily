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
