//! 任务冲突检测 — 移植自 `meeting_transcribe/assistant/tools/conflict.py`。

use chrono::NaiveDate;

use crate::minutes::llm::TaskItem;

/// 一条冲突；字段含义照 Python 的冲突字典。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// Python 的 `type` 键（Rust 关键字，改名 kind）。
    pub kind: &'static str,
    /// `"high"` / `"medium"`
    pub severity: &'static str,
    pub description: String,
    pub tasks: Vec<String>,
}

/// 照 `conflict.py:20-105`。键名归一化（`task_name` / `任务名称`）由
/// `TaskItem` 的 serde alias 完成，这里无需再补。
pub fn detect_conflicts(tasks: &[TaskItem]) -> Vec<Conflict> {
    let mut conflicts = Vec::new();

    // 1. 同一责任人 + 同一截止日期
    let mut same_deadline: Vec<((String, String), Vec<usize>)> = Vec::new();
    for (i, task) in tasks.iter().enumerate() {
        let person = task.owner.trim();
        let deadline = task.deadline.trim();
        if person.is_empty() || deadline.is_empty() {
            continue;
        }
        let key = (person.to_string(), deadline.to_string());
        match same_deadline
            .iter_mut()
            .find(|(existing, _)| *existing == key)
        {
            Some((_, indexes)) => indexes.push(i),
            None => same_deadline.push((key, vec![i])),
        }
    }
    for ((person, deadline), indexes) in &same_deadline {
        if indexes.len() > 1 {
            conflicts.push(Conflict {
                kind: "same_person_same_deadline",
                severity: "high",
                description: format!(
                    "「{person}」在 {deadline} 同时有 {} 个任务待完成",
                    indexes.len()
                ),
                tasks: indexes
                    .iter()
                    .map(|&i| tasks[i].task_name.clone())
                    .collect(),
            });
        }
    }

    // 2. 同一责任人 + 相近截止日期（1-3 天）
    let mut person_tasks: Vec<(String, Vec<(NaiveDate, usize)>)> = Vec::new();
    for (i, task) in tasks.iter().enumerate() {
        let person = task.owner.trim();
        let deadline = task.deadline.trim();
        if person.is_empty() || deadline.is_empty() {
            continue;
        }
        if let Ok(date) = NaiveDate::parse_from_str(deadline, "%Y-%m-%d") {
            match person_tasks.iter_mut().find(|(name, _)| name == person) {
                Some((_, list)) => list.push((date, i)),
                None => person_tasks.push((person.to_string(), vec![(date, i)])),
            }
        }
    }

    // Python 用 (person, 日期i, 日期j) 去重：同一日期的多个任务会重复命中同一对日期
    let mut reported: Vec<(String, NaiveDate, NaiveDate)> = Vec::new();
    for (person, list) in &mut person_tasks {
        list.sort_by_key(|(date, _)| *date);
        for i in 0..list.len() {
            for j in (i + 1)..list.len() {
                let days_diff = (list[j].0 - list[i].0).num_days().abs();
                if !(1..=3).contains(&days_diff) {
                    continue;
                }
                let key = (person.clone(), list[i].0, list[j].0);
                if reported.contains(&key) {
                    continue;
                }
                reported.push(key);
                conflicts.push(Conflict {
                    kind: "same_person_close_deadline",
                    severity: "medium",
                    description: format!(
                        "\"{person}\" 在 {} 和 {} 有连续任务（间隔 {days_diff} 天）",
                        list[i].0, list[j].0
                    ),
                    tasks: vec![
                        tasks[list[i].1].task_name.clone(),
                        tasks[list[j].1].task_name.clone(),
                    ],
                });
            }
        }
    }

    conflicts
}

/// 照 `conflict.py:108-144`；无冲突返回空串。
pub fn format_conflict_report(conflicts: &[Conflict]) -> String {
    if conflicts.is_empty() {
        return String::new();
    }

    let high: Vec<&Conflict> = conflicts.iter().filter(|c| c.severity == "high").collect();
    let medium: Vec<&Conflict> = conflicts
        .iter()
        .filter(|c| c.severity == "medium")
        .collect();

    let mut lines = vec![
        "## [任务冲突检测]".to_string(),
        String::new(),
        format!(
            "共发现 {} 处冲突（高: {} | 中: {}）",
            conflicts.len(),
            high.len(),
            medium.len()
        ),
        String::new(),
    ];

    if !high.is_empty() {
        lines.push("### [高优先级冲突]".to_string());
        lines.push("| # | 问题描述 | 涉及任务 |".to_string());
        lines.push("|---|----------|----------|".to_string());
        for (i, c) in high.iter().enumerate() {
            lines.push(format!(
                "| {} | {} | {} |",
                i + 1,
                c.description,
                c.tasks.join("、")
            ));
        }
        lines.push(String::new());
    }

    if !medium.is_empty() {
        lines.push("### [中优先级冲突]".to_string());
        lines.push("| # | 问题描述 | 涉及任务 |".to_string());
        lines.push("|---|----------|----------|".to_string());
        for (i, c) in medium.iter().enumerate() {
            lines.push(format!(
                "| {} | {} | {} |",
                i + 1,
                c.description,
                c.tasks.join("、")
            ));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

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
