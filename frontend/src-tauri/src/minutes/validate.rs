//! 行动项质量校验 — 移植自 `meeting_transcribe/assistant/tools/validator.py`。
//!
//! `format_validation_report` 不在这里移植：07 质检报告由 Task 7 从结构化结果渲染。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minutes::llm::TaskItem;
    use chrono::NaiveDate;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 4).unwrap() // 周二
    }

    #[test]
    fn normalizes_participants_and_deadline() {
        let t = TaskItem {
            task_name: "跟进".into(),
            owner: "许日政".into(),
            participants: "杨朋，张三".into(),
            deadline: "本周五".into(),
            ..Default::default()
        };
        let (fixed, log) = auto_fix_task(&t, today());
        assert_eq!(fixed.participants, "杨朋,张三");
        assert_eq!(fixed.deadline, "2026-08-07");
        assert_eq!(log.len(), 1);
        // 顺序照 Python fix_log：先日期、后参与人
        assert!(log[0].changes[0].starts_with("日期:"));
        assert_eq!(log[0].changes[1], "参与人标点已统一");
    }

    #[test]
    fn empty_owner_is_error_but_unknown_owner_is_warning() {
        let members = vec!["许日政".to_string()];
        let bad = validate_tasks(
            &[TaskItem {
                task_name: "a".into(),
                owner: "".into(),
                ..Default::default()
            }],
            &members,
            today(),
        );
        assert!(!bad.pass);

        let unknown = validate_tasks(
            &[TaskItem {
                task_name: "a".into(),
                owner: "李四".into(),
                deadline: "2026-08-10".into(),
                ..Default::default()
            }],
            &members,
            today(),
        );
        assert!(unknown.pass);
        assert!(unknown
            .issues
            .iter()
            .any(|i| i.level == "warning" && i.field == "责任人"));
    }

    #[test]
    fn multiple_owners_is_error() {
        let out = validate_tasks(
            &[TaskItem {
                task_name: "a".into(),
                owner: "张三,李四".into(),
                ..Default::default()
            }],
            &[],
            today(),
        );
        assert!(!out.pass);
    }

    #[test]
    fn empty_tasks_pass_with_placeholder_info() {
        let out = validate_tasks(&[], &[], today());
        assert!(out.pass);
        assert_eq!(out.issues.len(), 1);
        assert_eq!(out.issues[0].field, "行动项");
    }

    #[test]
    fn weekend_deadline_is_info_not_error() {
        let out = validate_tasks(
            &[TaskItem {
                task_name: "确认方案".into(),
                owner: "许日政".into(),
                deadline: "2026-08-08".into(), // 周六
                ..Default::default()
            }],
            &["许日政".to_string()],
            today(),
        );
        assert!(out.pass);
        assert!(out.issues.iter().any(|i| i.level == "info"
            && i.field == "截止日期"
            && i.message.contains("周末")));
    }

    #[test]
    fn normalizes_owner_separators() {
        let t = TaskItem {
            task_name: "跟进".into(),
            owner: "张三/李四".into(),
            ..Default::default()
        };
        let (fixed, _) = auto_fix_task(&t, today());
        assert_eq!(fixed.owner, "张三,李四");
    }

    #[test]
    fn similarity_matches_python_sequence_matcher() {
        // 期望值由 Python difflib.SequenceMatcher.ratio() 实算
        assert!((similarity("跟进A", "跟进B") - 0.666_666_666_666_666_6).abs() < 1e-12);
        assert!(
            (similarity("精锐线条灯评估覆膜板辊压方案", "精锐线条灯评估覆膜板辊压方案A")
                - 0.965_517_241_379_310_4)
                .abs()
                < 1e-12
        );
        assert!((similarity("评估覆膜板", "评估覆膜板辊压") - 0.833_333_333_333_333_4).abs() < 1e-12);
        assert_eq!(similarity("", ""), 1.0);
        assert_eq!(similarity("abc", "xyz"), 0.0);
    }
}
