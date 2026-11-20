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
