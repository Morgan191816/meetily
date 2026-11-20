//! 任务冲突检测 — 移植自 `meeting_transcribe/assistant/tools/conflict.py`。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minutes::llm::TaskItem;

    fn task(name: &str, owner: &str, deadline: &str) -> TaskItem {
        TaskItem {
            task_name: name.into(),
            owner: owner.into(),
            deadline: deadline.into(),
            ..Default::default()
        }
    }

    #[test]
    fn detects_same_owner_same_deadline_cluster() {
        let tasks = vec![
            task("跟进A", "许日政", "2026-08-07"),
            task("跟进B", "许日政", "2026-08-07"),
        ];
        let conflicts = detect_conflicts(&tasks);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].severity, "high");
        assert_eq!(conflicts[0].tasks, vec!["跟进A", "跟进B"]);
    }

    #[test]
    fn detects_close_deadlines_within_three_days() {
        let tasks = vec![
            task("跟进A", "许日政", "2026-08-07"),
            task("跟进B", "许日政", "2026-08-09"),
        ];
        let conflicts = detect_conflicts(&tasks);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].kind, "same_person_close_deadline");
        assert_eq!(conflicts[0].severity, "medium");
    }

    #[test]
    fn four_days_apart_is_not_a_conflict() {
        let tasks = vec![
            task("跟进A", "许日政", "2026-08-07"),
            task("跟进B", "许日政", "2026-08-11"),
        ];
        assert!(detect_conflicts(&tasks).is_empty());
    }

    #[test]
    fn report_renders_sections_and_empty_report_is_blank() {
        assert_eq!(format_conflict_report(&[]), "");
        let tasks = vec![
            task("跟进A", "许日政", "2026-08-07"),
            task("跟进B", "许日政", "2026-08-07"),
            task("跟进C", "许日政", "2026-08-09"),
        ];
        let md = format_conflict_report(&detect_conflicts(&tasks));
        assert!(md.starts_with("## [任务冲突检测]"));
        assert!(md.contains("### [高优先级冲突]"));
        assert!(md.contains("### [中优先级冲突]"));
        assert!(md.contains("跟进A、跟进B"));
        assert!(md.contains("共发现 2 处冲突（高: 1 | 中: 1）"));
    }
}
