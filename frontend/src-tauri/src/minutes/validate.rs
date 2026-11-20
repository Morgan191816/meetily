//! 行动项质量校验 — 移植自 `meeting_transcribe/assistant/tools/validator.py`。
//!
//! `format_validation_report` 不在这里移植：07 质检报告由 Task 7 从结构化结果渲染。

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};

use crate::minutes::dates::parse_date;
use crate::minutes::llm::TaskItem;

/// 照 `validator.py:22-28`；Python 列表里「确认」重复一次，去重后 39 个。
const ACTION_VERBS: [&str; 39] = [
    "评估", "确认", "完成", "启动", "提交", "制定", "开发", "测试", "验证", "协调", "对接", "整理",
    "更新", "发布", "跟进", "落实", "调研", "分析", "设计", "实现", "部署", "检查", "审核", "处理",
    "修复", "优化", "重构", "迁移", "编写", "沟通", "输出", "收集", "汇总", "统计", "核对", "审批",
    "下达", "通知", "安排",
];

/// 照 `validator.py:32-38`，格式: (月, 日)。
const STATIC_HOLIDAYS: [(u32, u32); 5] = [(1, 1), (5, 1), (10, 1), (10, 2), (10, 3)];

/// 单条校验结论；字段含义照 `validator.py` 的 issue 字典。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// 通常是 1 起始序号；跨任务规则为 "-" 或 "1,2"。
    pub task_index: String,
    pub task_name: String,
    /// `"error"` / `"warning"` / `"info"`
    pub level: &'static str,
    pub field: &'static str,
    pub message: String,
}

/// 自动修复记录；字段含义照 Python `fix_log` 条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixLog {
    /// 1 起始序号；`auto_fix_task` 不掌握位置，返回 0，由 `validate_tasks` 回填。
    pub task_index: usize,
    pub task_name: String,
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationOutcome {
    pub fixed_tasks: Vec<TaskItem>,
    pub issues: Vec<Issue>,
    pub pass: bool,
    pub fix_log: Vec<FixLog>,
}

impl Default for ValidationOutcome {
    fn default() -> Self {
        // 空结果视为通过，与 Python 空任务列表的 pass=True 一致
        Self {
            fixed_tasks: Vec::new(),
            issues: Vec::new(),
            pass: true,
            fix_log: Vec::new(),
        }
    }
}

/// 照 `validator.py:55-86` 自动修复常见格式问题。
///
/// 键名双向补齐（`task_name` / `任务名称`）由 `TaskItem` 的 serde alias 完成，
/// 结构体里只有 `task_name` 一个字段，这里无需再补。
pub fn auto_fix_task(task: &TaskItem, today: NaiveDate) -> (TaskItem, Vec<FixLog>) {
    let mut fixed = task.clone();
    let mut changes = Vec::new();

    // 截止日期自然语言 → YYYY-MM-DD
    let deadline = task.deadline.trim();
    if !deadline.is_empty() {
        if let Some(parsed) = parse_date(deadline, today) {
            let parsed = parsed.format("%Y-%m-%d").to_string();
            if parsed != deadline {
                fixed.deadline = parsed.clone();
                // 与 Python fix_log 一致：消息用原始值（含空白）
                changes.push(format!("日期: \"{}\" → \"{}\"", task.deadline, parsed));
            }
        }
    }

    // 参与人中文逗号 → 英文逗号
    if fixed.participants.contains('，') {
        fixed.participants = fixed.participants.replace('，', ",");
        changes.push("参与人标点已统一".to_string());
    }

    // 责任人分隔符统一（Python 只替换命中的第一种分隔符）
    for sep in ['，', '、', '/', '\\'] {
        if task.owner.contains(sep) {
            fixed.owner = task.owner.replace(sep, ",");
            break;
        }
    }

    let log = if changes.is_empty() {
        Vec::new()
    } else {
        vec![FixLog {
            task_index: 0,
            task_name: fixed.task_name.clone(),
            changes,
        }]
    };
    (fixed, log)
}

/// 照 `validator.py:41-47`：先周末、后法定节假日。
fn weekend_or_holiday(d: NaiveDate) -> Option<&'static str> {
    if d.weekday().num_days_from_monday() >= 5 {
        return Some("周末");
    }
    if STATIC_HOLIDAYS.contains(&(d.month(), d.day())) {
        return Some("法定节假日");
    }
    None
}

/// 照 `validator.py:89-190`，返回 `(level, field, message)` 三元组。
fn validate_task(
    task: &TaskItem,
    members: &[String],
    today: NaiveDate,
) -> Vec<(&'static str, &'static str, String)> {
    let mut issues = Vec::new();

    // 1. 责任人是否为空；2. 责任人是否唯一
    let owner = task.owner.trim();
    if owner.is_empty() {
        issues.push(("error", "责任人", "责任人不能为空".to_string()));
    } else if [",", "，", "、", "/", "\\"]
        .iter()
        .any(|sep| owner.contains(sep))
    {
        issues.push((
            "error",
            "责任人",
            format!("责任人必须唯一，当前含多人: \"{owner}\""),
        ));
    }

    // 3. 责任人是否在 OA 名单中（名单为空时跳过）
    if !members.is_empty() && !owner.is_empty() && !members.iter().any(|m| m == owner) {
        issues.push((
            "warning",
            "责任人",
            format!("\"{owner}\" 不在 OA 人员名单中，请确认"),
        ));
    }

    // 4. 截止日期格式与合理性
    let deadline = task.deadline.trim();
    if deadline.is_empty() {
        issues.push(("error", "截止日期", "截止日期不能为空".to_string()));
    } else {
        match NaiveDate::parse_from_str(deadline, "%Y-%m-%d") {
            Ok(d) => {
                if d < today {
                    issues.push((
                        "warning",
                        "截止日期",
                        format!("截止日期 {deadline} 已过期（当前 {today}）"),
                    ));
                } else if (d - today).num_days() > 365 {
                    issues.push((
                        "warning",
                        "截止日期",
                        format!("截止日期 {deadline} 超过一年，请确认"),
                    ));
                }

                // 4a. 截止日期在周末或节假日
                if let Some(reason) = weekend_or_holiday(d) {
                    issues.push((
                        "info",
                        "截止日期",
                        format!("截止日期 {deadline} 是{reason}，建议调整"),
                    ));
                }
            }
            Err(_) => match parse_date(deadline, today) {
                Some(parsed) => issues.push((
                    "info",
                    "截止日期",
                    format!("自动转换: \"{deadline}\" → {}", parsed.format("%Y-%m-%d")),
                )),
                None => issues.push((
                    "error",
                    "截止日期",
                    format!("日期格式无法识别: \"{deadline}\""),
                )),
            },
        }
    }

    // 5. 任务名称是否动词开头；5a. 任务粒度
    let task_name = task.task_name.trim();
    if task_name.is_empty() {
        issues.push(("error", "任务名称", "任务名称不能为空".to_string()));
    } else {
        if !ACTION_VERBS.iter().any(|v| task_name.starts_with(v)) {
            let first_char = task_name.chars().next().unwrap_or(' ');
            issues.push((
                "info",
                "任务名称",
                format!("建议以动词开头（当前首字: \"{first_char}\"），如: 评估/确认/完成/启动/提交/制定"),
            ));
        }

        let len = task_name.chars().count();
        if len < 6 {
            issues.push((
                "info",
                "任务名称",
                format!("任务名称过短（{len}字），可能过于粗放"),
            ));
        } else if len > 30 {
            issues.push((
                "info",
                "任务名称",
                format!("任务名称偏长（{len}字），可能过于琐碎"),
            ));
        }
    }

    // 6. 参与人格式
    if task.participants.trim().contains('，') {
        issues.push((
            "info",
            "参与人",
            "参与人使用了中文逗号，建议统一为英文逗号".to_string(),
        ));
    }

    // 7. OA 全字段校验
    if task.description.trim().is_empty() {
        issues.push(("info", "描述", "描述为空，建议补充任务背景".to_string()));
    }
    if task.risk.trim().is_empty() {
        issues.push(("info", "风险点", "风险点为空，建议标注已知风险".to_string()));
    }
    if task.difficulty.trim().is_empty() {
        issues.push((
            "info",
            "难度点",
            "难度点为空，建议标注技术/资源难度".to_string(),
        ));
    }
    if task.help_needed.trim().is_empty() {
        issues.push((
            "info",
            "需协助点",
            "需协助点为空，建议标注跨部门协作需求".to_string(),
        ));
    }

    issues
}

/// 照 `validator.py:193-357` 批量校验，自动修复恒开。
pub fn validate_tasks(
    tasks: &[TaskItem],
    members: &[String],
    today: NaiveDate,
) -> ValidationOutcome {
    // 极端情况 1：空任务列表
    if tasks.is_empty() {
        return ValidationOutcome {
            fixed_tasks: Vec::new(),
            issues: vec![Issue {
                task_index: "-".to_string(),
                task_name: "-".to_string(),
                level: "info",
                field: "行动项",
                message: "未提取到行动项，请确认会议记录中是否有明确的任务分配".to_string(),
            }],
            pass: true,
            fix_log: Vec::new(),
        };
    }

    // 自动修复
    let mut fixed_tasks = Vec::with_capacity(tasks.len());
    let mut fix_log = Vec::new();
    for (i, task) in tasks.iter().enumerate() {
        let (fixed, mut log) = auto_fix_task(task, today);
        for entry in &mut log {
            entry.task_index = i + 1;
        }
        fix_log.extend(log);
        fixed_tasks.push(fixed);
    }

    let mut issues = Vec::new();

    // 逐项校验
    for (i, task) in fixed_tasks.iter().enumerate() {
        for (level, field, message) in validate_task(task, members, today) {
            issues.push(Issue {
                task_index: (i + 1).to_string(),
                task_name: task.task_name.clone(),
                level,
                field,
                message,
            });
        }
    }

    // 8. 工作量评估（自适应阈值）
    let mut person_counts: Vec<(String, usize)> = Vec::new();
    for task in &fixed_tasks {
        let person = task.owner.trim();
        if !person.is_empty() && members.iter().any(|m| m == person) {
            match person_counts.iter_mut().find(|(name, _)| name == person) {
                Some((_, count)) => *count += 1,
                None => person_counts.push((person.to_string(), 1)),
            }
        }
    }

    if !person_counts.is_empty() {
        let avg = person_counts.iter().map(|(_, count)| *count).sum::<usize>() as f64
            / person_counts.len() as f64;
        let task_count = fixed_tasks.len();
        let threshold = if task_count <= 10 {
            avg * 2.0
        } else if task_count <= 20 {
            avg * 2.5
        } else {
            avg * 3.0
        };

        // 稳定排序保留首次出现顺序，对应 Counter.most_common 的并列顺序
        person_counts.sort_by(|a, b| b.1.cmp(&a.1));
        for (person, count) in &person_counts {
            if *count as f64 >= threshold && *count >= 3 {
                issues.push(Issue {
                    task_index: "-".to_string(),
                    task_name: "-".to_string(),
                    level: "warning",
                    field: "工作量",
                    message: format!(
                        "\"{person}\" 承担 {count} 项任务，远超平均 {avg:.1} 项，建议评估是否过载"
                    ),
                });
            }
        }
    }

    // 9. 任务重复检测
    for i in 0..fixed_tasks.len() {
        for j in (i + 1)..fixed_tasks.len() {
            let sim = similarity(&fixed_tasks[i].task_name, &fixed_tasks[j].task_name);
            if sim > 0.8 {
                issues.push(Issue {
                    task_index: format!("{},{}", i + 1, j + 1),
                    task_name: format!(
                        "{} / {}",
                        fixed_tasks[i].task_name, fixed_tasks[j].task_name
                    ),
                    level: "warning",
                    field: "任务重复",
                    message: format!(
                        "两项任务高度相似（相似度 {:.0}%），请确认是否重复",
                        sim * 100.0
                    ),
                });
            }
        }
    }

    // 10. 跨任务一致性：相同责任人的项目写法
    let mut person_prefixes: Vec<(String, Vec<String>)> = Vec::new();
    for task in &fixed_tasks {
        let person = task.owner.trim();
        let name = task.task_name.trim();
        if person.is_empty() || name.is_empty() {
            continue;
        }
        let prefix: String = name.chars().take(4).collect();
        match person_prefixes.iter_mut().find(|(p, _)| p == person) {
            Some((_, prefixes)) => {
                if !prefixes.contains(&prefix) {
                    prefixes.push(prefix);
                }
            }
            None => person_prefixes.push((person.to_string(), vec![prefix])),
        }
    }

    for (person, prefixes) in &person_prefixes {
        for i in 0..prefixes.len() {
            for j in (i + 1)..prefixes.len() {
                if similarity(&prefixes[i], &prefixes[j]) > 0.6 {
                    issues.push(Issue {
                        task_index: "-".to_string(),
                        task_name: "-".to_string(),
                        level: "info",
                        field: "命名一致性",
                        message: format!(
                            "\"{person}\" 的任务中项目写法不一致: \"{}\" vs \"{}\"",
                            prefixes[i], prefixes[j]
                        ),
                    });
                }
            }
        }
    }

    let pass = !issues.iter().any(|issue| issue.level == "error");
    ValidationOutcome {
        fixed_tasks,
        issues,
        pass,
        fix_log,
    }
}

/// Python `difflib.SequenceMatcher.ratio()` 的等价实现（无 junk 判定）。
///
/// shortcut: 未实现 autojunk（仅当 b 超过 200 字符才生效），任务名不会达到该长度。
fn similarity(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let matched: usize = matching_blocks(&a, &b)
        .iter()
        .map(|&(_, _, size)| size)
        .sum();
    2.0 * matched as f64 / total as f64
}

/// 照 `SequenceMatcher.get_matching_blocks()`：递归取最长匹配块，最后按位置排序。
fn matching_blocks(a: &[char], b: &[char]) -> Vec<(usize, usize, usize)> {
    let mut queue = vec![(0usize, a.len(), 0usize, b.len())];
    let mut blocks = Vec::new();
    while let Some((alo, ahi, blo, bhi)) = queue.pop() {
        let (i, j, k) = find_longest_match(a, b, alo, ahi, blo, bhi);
        if k > 0 {
            blocks.push((i, j, k));
            if alo < i && blo < j {
                queue.push((alo, i, blo, j));
            }
            if i + k < ahi && j + k < bhi {
                queue.push((i + k, ahi, j + k, bhi));
            }
        }
    }
    blocks.sort_unstable();
    blocks
}

/// 照 `SequenceMatcher.find_longest_match()`：最长匹配块，并列时取 a 中最早、再取 b 中最早。
fn find_longest_match(
    a: &[char],
    b: &[char],
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> (usize, usize, usize) {
    let mut b2j: HashMap<char, Vec<usize>> = HashMap::new();
    for (j, &ch) in b.iter().enumerate().take(bhi).skip(blo) {
        b2j.entry(ch).or_default().push(j);
    }

    let mut besti = alo;
    let mut bestj = blo;
    let mut bestsize = 0usize;
    let mut j2len: HashMap<usize, usize> = HashMap::new();

    for (i, &ch) in a.iter().enumerate().take(ahi).skip(alo) {
        let mut newj2len: HashMap<usize, usize> = HashMap::new();
        if let Some(positions) = b2j.get(&ch) {
            for &j in positions {
                let k = if j == 0 {
                    1
                } else {
                    j2len.get(&(j - 1)).copied().unwrap_or(0) + 1
                };
                newj2len.insert(j, k);
                if k > bestsize {
                    besti = i + 1 - k;
                    bestj = j + 1 - k;
                    bestsize = k;
                }
            }
        }
        j2len = newj2len;
    }

    // Python 的向前/向后扩展循环只对 junk 元素生效，无 junk 时为 no-op，省略。
    (besti, bestj, bestsize)
}

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
        assert!(out
            .issues
            .iter()
            .any(|i| i.level == "info" && i.field == "截止日期" && i.message.contains("周末")));
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
            (similarity(
                "精锐线条灯评估覆膜板辊压方案",
                "精锐线条灯评估覆膜板辊压方案A"
            ) - 0.965_517_241_379_310_4)
                .abs()
                < 1e-12
        );
        assert!(
            (similarity("评估覆膜板", "评估覆膜板辊压") - 0.833_333_333_333_333_4).abs() < 1e-12
        );
        assert_eq!(similarity("", ""), 1.0);
        assert_eq!(similarity("abc", "xyz"), 0.0);
    }
}
