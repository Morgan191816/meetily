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
