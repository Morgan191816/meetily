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
    let draft = input.draft;
    let fixed = &input.validation.fixed_tasks;
    let claim_map = build_claim_map(input.claims);
    let mut lines: Vec<String> = Vec::new();

    // ── 1. YAML frontmatter（Obsidian 属性，Dataview 可查询） ──
    lines.push("---".to_string());
    lines.push("type: meeting".to_string());
    lines.push(format!("title: {}", yaml_quote(&input.title)));
    lines.push(format!("date: {}", input.meeting_date));
    lines.push(format!("project: {}", yaml_quote(&input.project)));
    lines.push(format!("attendees: [{}]", input.attendees.join(", ")));
    lines.push(format!("action_items: {}", fixed.len()));
    // TaskItem 没有状态字段，全部按 Python 的默认「未开始」计，未完成数即总数
    lines.push(format!("open_action_items: {}", fixed.len()));
    lines.push(format!("source: {}", yaml_quote(&input.source)));
    lines.push("---".to_string());
    lines.push(String::new());

    // ── 2. 标题块（标题 + 日期 + 参会人员 + 待确认人员） ──
    lines.push(format!("# 会议纪要：{}", input.title));
    lines.push(format!("日期：{}", input.meeting_date));
    if !input.attendees.is_empty() {
        lines.push(format!("参会人员：{}", input.attendees.join(",")));
    }
    if !input.unmatched_attendees.is_empty() {
        lines.push(format!(
            "待确认人员：{}",
            input.unmatched_attendees.join(",")
        ));
    }
    lines.push(String::new());

    // ── 3. 核心结论（结论先行，金字塔原理） ──
    let mut core: Vec<&str> = draft
        .decisions
        .iter()
        .map(|d| d.content.as_str())
        .filter(|content| !content.is_empty())
        .take(3)
        .collect();
    if core.len() < 3 {
        for topic in &draft.topics {
            if !topic.conclusion.is_empty() && !core.contains(&topic.conclusion.as_str()) {
                core.push(&topic.conclusion);
            }
            if core.len() >= 3 {
                break;
            }
        }
    }
    if !core.is_empty() {
        lines.push("## 核心结论".to_string());
        for content in &core {
            lines.push(format!("- {content}"));
        }
        lines.push(String::new());
    }

    // ── 4. 会议议题（议题名 + 讨论要点 + 结论 + 分歧点） ──
    lines.push("## 会议议题".to_string());
    if !draft.topics.is_empty() {
        for (offset, topic) in draft.topics.iter().enumerate() {
            let index = offset + 1;
            let title = if topic.title.is_empty() {
                format!("议题 {index}")
            } else {
                topic.title.clone()
            };
            lines.push(format!("{index}. {title}"));
            for (label, value) in [
                ("讨论要点", &topic.discussion),
                ("结论", &topic.conclusion),
                ("分歧点", &topic.disagreement),
            ] {
                if !value.is_empty() {
                    lines.push(format!("   - {label}：{value}"));
                }
            }
        }
    } else {
        lines.push("（未提取到明确议题）".to_string());
    }
    lines.push(String::new());

    // ── 5. 决策事项（编号 + 内容 + 依据 + 生效时间） ──
    lines.push("## 决策事项".to_string());
    if !draft.decisions.is_empty() {
        for (offset, decision) in draft.decisions.iter().enumerate() {
            let content = strip_inference_marker(&decision.content);
            let marker = verdict_marker(claim_map.get(content).copied());
            let mut detail = Vec::new();
            if !decision.basis.is_empty() {
                detail.push(format!("依据：{}", decision.basis));
            }
            if !decision.effective_date.is_empty() {
                detail.push(format!("生效：{}", decision.effective_date));
            }
            let suffix = if detail.is_empty() {
                String::new()
            } else {
                format!("（{}）", detail.join("；"))
            };
            lines.push(format!(
                "- 决策-{:02}：{content}{marker}{suffix}",
                offset + 1
            ));
        }
    } else {
        lines.push("本次会议无明确决策事项。".to_string());
    }
    lines.push(String::new());

    // ── 6. 行动项（含状态列） ──
    lines.push("## 行动项".to_string());
    if !fixed.is_empty() {
        lines.push("| 序号 | 任务名称 | 责任人 | 参与人 | 截止日期 | 状态 |".to_string());
        lines.push("|------|----------|--------|--------|----------|------|".to_string());
        for (offset, task) in fixed.iter().enumerate() {
            let name = strip_inference_marker(&task.task_name);
            let marker = verdict_marker(claim_map.get(name).copied());
            lines.push(format!(
                "| {} | {name}{marker} | {} | {} | {} | 未开始 |",
                offset + 1,
                task.owner,
                task.participants,
                task.deadline
            ));
        }
    } else {
        lines.push("本次会议未提取到行动项。".to_string());
    }
    lines.push(String::new());

    // ── 7. 风险与阻碍（风险 → 严重度 → 缓解措施 → 负责人） ──
    lines.push("## 风险与阻碍".to_string());
    if !draft.risks.is_empty() {
        for risk in &draft.risks {
            let description = strip_inference_marker(&risk.description);
            let marker = verdict_marker(claim_map.get(description).copied());
            let mut detail = Vec::new();
            if !risk.severity.is_empty() {
                detail.push(format!("严重度：{}", risk.severity));
            }
            if !risk.mitigation.is_empty() {
                detail.push(format!("缓解：{}", risk.mitigation));
            }
            if !risk.owner.is_empty() {
                detail.push(format!("负责人：{}", risk.owner));
            }
            let suffix = if detail.is_empty() {
                String::new()
            } else {
                format!("（{}）", detail.join("；"))
            };
            lines.push(format!("- {description}{marker}{suffix}"));
        }
    } else {
        lines.push("无。".to_string());
    }
    lines.push(String::new());

    // ── 8. 待决问题（非空才出） ──
    if !draft.open_issues.is_empty() {
        lines.push("## 待决问题".to_string());
        for issue in &draft.open_issues {
            lines.push(format!("- {issue}"));
        }
        lines.push(String::new());
    }

    // ── 9. 下次会议 ──
    if let Some(next) = &draft.next_meeting {
        if !next.date.is_empty() || !next.agenda.is_empty() {
            lines.push("## 下次会议".to_string());
            if !next.date.is_empty() {
                lines.push(format!(
                    "- 日期：{}",
                    normalize_meeting_date(&next.date, input.today)
                ));
            }
            if !next.agenda.is_empty() {
                lines.push(format!("- 议程：{}", next.agenda));
            }
            lines.push(String::new());
        }
    }

    // ── 10. 处理状态（透明可审计） ──
    lines.push("## 处理状态".to_string());
    lines.push(format!(
        "- 溯源校验：{}",
        if input.traceability_pass {
            "✅ 通过"
        } else {
            "⚠️ 有项需核实"
        }
    ));
    lines.push(format!(
        "- 质量自检：{}",
        if input.validation.pass {
            "✅ 通过"
        } else {
            "⚠️ 有错误项"
        }
    ));
    lines.push(String::new());

    // ── 11. 溯源出处（决策/行动项的证据，AI 纪要证据模式） ──
    let mut evidence: Vec<(&str, &str, &ClaimResult)> = Vec::new();
    for decision in &draft.decisions {
        let text = strip_inference_marker(&decision.content);
        if !text.is_empty() {
            if let Some(info) = claim_map.get(text).copied() {
                evidence.push(("决策", text, info));
            }
        }
    }
    for task in fixed {
        let name = strip_inference_marker(&task.task_name);
        if !name.is_empty() {
            if let Some(info) = claim_map.get(name).copied() {
                evidence.push(("行动项", name, info));
            }
        }
    }
    if !evidence.is_empty() {
        lines.push("## 溯源出处".to_string());
        lines.push("<details>".to_string());
        lines.push("<summary>决策与行动项的证据出处（点击查看）</summary>".to_string());
        lines.push(String::new());
        lines.push("| 类型 | 声明 | 判定 | 置信度 | 原文依据 |".to_string());
        lines.push("|------|------|------|--------|----------|".to_string());
        for (kind, text, info) in &evidence {
            // Python 只转义出处片段里的 |，声明列原样输出（保持逐字一致）
            let snippet = info
                .sources
                .first()
                .map(|source| source.chars().take(60).collect::<String>())
                .unwrap_or_default()
                .replace('|', "｜");
            lines.push(format!(
                "| {kind} | {text} | {} | {} | {snippet} |",
                verdict_label(info.verdict),
                confidence_label(info.confidence)
            ));
        }
        lines.push(String::new());
        lines.push("</details>".to_string());
        lines.push(String::new());
    }

    // ── 12. 修订记录 ──
    lines.push("<details>".to_string());
    lines.push("<summary>修订记录（点击查看）</summary>".to_string());
    lines.push(String::new());
    if !input.validation.fix_log.is_empty() {
        lines.push("| 序号 | 任务 | 修正项 |".to_string());
        lines.push("|------|------|--------|".to_string());
        for (offset, log) in input.validation.fix_log.iter().enumerate() {
            lines.push(format!(
                "| {} | {} | {} |",
                offset + 1,
                log.task_name,
                log.changes.join("; ")
            ));
        }
    } else {
        lines.push("无自动修复。".to_string());
    }
    lines.push(String::new());
    lines.push("</details>".to_string());

    MinutesDocument {
        markdown: lines.join("\n"),
    }
}

/// 声明文本 → 溯源结果；重复文本后者覆盖（照 Python dict 赋值）。
fn build_claim_map(claims: &[ClaimResult]) -> HashMap<&str, &ClaimResult> {
    claims
        .iter()
        .map(|result| (result.claim.as_str(), result))
        .collect()
}

/// YAML 单行标量双引号转义（title/source 可能含 : , " 等）。
fn yaml_quote(text: &str) -> String {
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

/// 去掉声明文本开头的 [推论]/〔推论〕（避免与内联溯源标记重复）。
fn strip_inference_marker(text: &str) -> &str {
    for prefix in ["[推论]", "〔推论〕"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return rest.trim();
        }
    }
    text
}

fn verdict_marker(info: Option<&ClaimResult>) -> &'static str {
    match info.map(|result| result.verdict) {
        Some(Verdict::Verified) => "〔已溯源〕",
        Some(Verdict::Inferred) => "〔推论〕",
        Some(Verdict::Unsupported) => "〔待核实〕",
        None => "",
    }
}

fn verdict_label(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Verified => "✅ 已溯源",
        Verdict::Inferred => "⚠️ 推论",
        Verdict::Unsupported => "❌ 待核实",
    }
}

fn confidence_label(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::High => "高",
        Confidence::Medium => "中",
        Confidence::Low => "低",
    }
}

/// 自然语言日期归一化为 YYYY-MM-DD；无法解析保留原文。
fn normalize_meeting_date(text: &str, today: NaiveDate) -> String {
    parse_date(text, today)
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| text.to_string())
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
