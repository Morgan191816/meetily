//! 05 会议纪要渲染 — 移植自 `meeting_transcribe/assistant_bridge.py:331-540`。
//!
//! 12 个区块的顺序与文案逐行对照 Python。Python 里「rich 为空时回退 ctx 扁平字段」
//! 的分支不可达（扁平字段由同一 rich 派生），不移植。

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::minutes::dates::parse_date;
use crate::minutes::llm::MinutesDraft;
use crate::minutes::traceability::{ClaimResult, Confidence, Verdict};
use crate::minutes::validate::ValidationOutcome;

pub struct MinutesDocument {
    pub markdown: String,
}

/// 渲染所需的全部上下文；`draft` 为 step2 富结构，`validation` 携带修复后的任务与
/// 自检结论，`claims` 为桥接层重跑的溯源结果（照 `_build_claim_map` 的声明集合）。
pub struct MinutesRenderInput<'a> {
    pub title: String,
    pub meeting_date: String,
    /// 输出目录名（Python `output_dir.name`）。
    pub project: String,
    pub source: String,
    /// 注入的参考日，用于下次会议日期归一化。
    pub today: NaiveDate,
    pub attendees: Vec<String>,
    pub unmatched_attendees: Vec<String>,
    pub draft: &'a MinutesDraft,
    pub validation: &'a ValidationOutcome,
    pub claims: &'a [ClaimResult],
    pub traceability_pass: bool,
}

pub fn render_minutes(input: &MinutesRenderInput<'_>) -> MinutesDocument {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minutes::llm::{Decision, NextMeeting, Risk, TaskItem, Topic};
    use crate::minutes::traceability::ClaimResult;
    use crate::minutes::validate::FixLog;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 4).unwrap()
    }

    fn claim(text: &str, verdict: Verdict) -> ClaimResult {
        ClaimResult {
            claim: text.to_string(),
            verdict,
            confidence: Confidence::High,
            sources: vec!["会上确认华硕不落总灯已经结案了".to_string()],
        }
    }

    fn task() -> TaskItem {
        TaskItem {
            task_name: "跟进冯亚项目进度".to_string(),
            owner: "杨朋".to_string(),
            participants: "许日政".to_string(),
            deadline: "2026-08-07".to_string(),
            ..Default::default()
        }
    }

    fn render_fixture() -> MinutesDocument {
        render_with_tasks(vec![task()])
    }

    fn render_with_tasks(tasks: Vec<TaskItem>) -> MinutesDocument {
        let draft = MinutesDraft {
            title: "路灯会议".to_string(),
            date: "2026-08-04".to_string(),
            decisions: vec![Decision {
                content: "华硕不落总灯已经结案".to_string(),
                basis: "会上确认".to_string(),
                effective_date: "2026-08-05".to_string(),
            }],
            topics: vec![Topic {
                title: "路灯进度".to_string(),
                discussion: "讨论了华硕项目".to_string(),
                conclusion: "继续推进".to_string(),
                disagreement: String::new(),
            }],
            risks: vec![Risk {
                description: "预算还没批".to_string(),
                severity: "高".to_string(),
                mitigation: "下周补报".to_string(),
                owner: "许日政".to_string(),
            }],
            open_issues: Vec::new(),
            next_meeting: Some(NextMeeting {
                date: "下周三".to_string(),
                agenda: "预算复审".to_string(),
            }),
        };
        let validation = ValidationOutcome {
            fixed_tasks: tasks,
            issues: Vec::new(),
            pass: true,
            fix_log: vec![FixLog {
                task_index: 1,
                task_name: "跟进冯亚项目进度".to_string(),
                changes: vec!["日期已归一化".to_string()],
            }],
        };
        let claims = vec![
            claim("华硕不落总灯已经结案", Verdict::Verified),
            claim("跟进冯亚项目进度", Verdict::Unsupported),
        ];
        render_minutes(&MinutesRenderInput {
            title: "路灯会议".to_string(),
            meeting_date: "2026-08-04".to_string(),
            project: "路灯项目".to_string(),
            source: "录音.mp3".to_string(),
            today: today(),
            attendees: vec!["许日政".to_string(), "杨朋".to_string()],
            unmatched_attendees: Vec::new(),
            draft: &draft,
            validation: &validation,
            claims: &claims,
            traceability_pass: false,
        })
    }

    #[test]
    fn renders_all_twelve_blocks_in_order() {
        let md = render_fixture().markdown;
        let order = [
            "---\ntype: meeting",
            "# 会议纪要：",
            "## 核心结论",
            "## 会议议题",
            "## 决策事项",
            "## 行动项",
            "## 风险与阻碍",
            "## 处理状态",
            "## 溯源出处",
            "<summary>修订记录",
        ];
        let mut cursor = 0;
        for marker in order {
            let pos = md[cursor..]
                .find(marker)
                .unwrap_or_else(|| panic!("缺少区块 {marker}"))
                + cursor;
            cursor = pos;
        }
    }

    #[test]
    fn action_table_has_six_columns_and_default_status() {
        let md = render_fixture().markdown;
        assert!(md.contains("| 序号 | 任务名称 | 责任人 | 参与人 | 截止日期 | 状态 |"));
        assert!(md.contains("| 未开始 |"));
    }

    #[test]
    fn inline_markers_follow_verdicts() {
        let md = render_fixture().markdown;
        assert!(md.contains("〔已溯源〕"));
        assert!(md.contains("〔待核实〕"));
    }

    #[test]
    fn omits_open_issues_section_when_empty() {
        assert!(!render_fixture().markdown.contains("## 待决问题"));
    }

    #[test]
    fn frontmatter_quotes_title() {
        assert!(render_fixture().markdown.contains("title: \"路灯会议\""));
    }

    #[test]
    fn decision_numbers_are_zero_padded() {
        let md = render_fixture().markdown;
        assert!(md.contains("- 决策-01：华硕不落总灯已经结案"));
        assert!(!md.contains("- 决策-1："));
    }

    #[test]
    fn empty_tasks_render_placeholder_instead_of_table() {
        let md = render_with_tasks(Vec::new()).markdown;
        assert!(md.contains("本次会议未提取到行动项。"));
        assert!(!md.contains("| 序号 | 任务名称 |"));
    }
}
