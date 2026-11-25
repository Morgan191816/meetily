//! 07 质检报告渲染 — 移植自 `assistant_bridge.py:543-577`、
//! `traceability.py:273-316`、`validator.py:360-408`、`conflict.py:108-144`。
//!
//! 结构照桥接层：四个 `##` 区块；各区块正文由对应的 Python `format_*_report`
//! 生成（其自身还带一个 `## [...]` 小标题，Python 07 产物就是这样嵌套的）。
//! 已裁定偏离：不输出「待确认沉淀候选」空节。

use crate::minutes::conflict::{format_conflict_report, Conflict};
use crate::minutes::traceability::{ClaimKind, ClaimResult, Confidence, Verdict};
use crate::minutes::validate::{Issue, ValidationOutcome};

/// 渲染 07 质检报告（溯源校验 + 质量自检 + 任务冲突检测）。
pub fn render_quality_report(
    trace: &[ClaimResult],
    validation: &ValidationOutcome,
    conflicts: &[Conflict],
) -> String {
    let mut lines = vec!["# 质检报告".to_string(), String::new()];

    lines.push("## 溯源校验".to_string());
    lines.push(if trace.is_empty() {
        "无需要校验的声明项".to_string()
    } else {
        format_traceability_report(trace)
    });
    lines.push(String::new());

    // Python 在无行动项时跳过质检（orchestrator.py:578-583）
    lines.push("## 质量自检".to_string());
    lines.push(if validation.fixed_tasks.is_empty() {
        "未提取到行动项，跳过质量自检".to_string()
    } else {
        format_validation_report(validation)
    });
    lines.push(String::new());

    lines.push("## 任务冲突检测".to_string());
    let conflict_report = format_conflict_report(conflicts);
    lines.push(if conflict_report.is_empty() {
        "未发现任务冲突。".to_string()
    } else {
        conflict_report
    });
    lines.push(String::new());

    lines.join("\n")
}

/// 照 `traceability.py:273-316`：统计行 + 三张列数不同的表。
fn format_traceability_report(trace: &[ClaimResult]) -> String {
    let verified: Vec<&ClaimResult> = trace
        .iter()
        .filter(|r| r.verdict == Verdict::Verified)
        .collect();
    let inferred: Vec<&ClaimResult> = trace
        .iter()
        .filter(|r| r.verdict == Verdict::Inferred)
        .collect();
    let unsupported: Vec<&ClaimResult> = trace
        .iter()
        .filter(|r| r.verdict == Verdict::Unsupported)
        .collect();

    let mut lines = vec![
        "## [溯源校验报告]".to_string(),
        String::new(),
        format!(
            "- ✅ 已验证: {} 项 | ⚠️ 推论: {} 项 | ❌ 无依据: {} 项",
            verified.len(),
            inferred.len(),
            unsupported.len()
        ),
        format!(
            "- **总体: {}**",
            if unsupported.is_empty() {
                "✅ 全部可溯源"
            } else {
                "❌ 存在无依据项，请核实"
            }
        ),
        String::new(),
    ];

    if !unsupported.is_empty() {
        lines.push("### ❌ 无依据（必须核实）".to_string());
        lines.push("| # | 类型 | 声明内容 | 判定理由 |".to_string());
        lines.push("|---|------|----------|----------|".to_string());
        for (i, r) in unsupported.iter().enumerate() {
            lines.push(format!(
                "| {} | {} | {} | {} |",
                i + 1,
                kind_label(r.kind),
                r.claim,
                r.reason
            ));
        }
        lines.push(String::new());
    }

    if !inferred.is_empty() {
        lines.push("### ⚠️ 推论（需确认）".to_string());
        lines.push("| # | 类型 | 声明内容 | 置信度 | 原文依据 |".to_string());
        lines.push("|---|------|----------|--------|----------|".to_string());
        for (i, r) in inferred.iter().enumerate() {
            // Python: r['sources'][0][:60] + '...'，无出处写「无」
            let source = match r.sources.first() {
                Some(s) => format!("{}...", s.chars().take(60).collect::<String>()),
                None => "无".to_string(),
            };
            lines.push(format!(
                "| {} | {} | {} | {} | {} |",
                i + 1,
                kind_label(r.kind),
                r.claim,
                confidence_label(r.confidence),
                source
            ));
        }
        lines.push(String::new());
    }

    if !verified.is_empty() {
        lines.push("### ✅ 已验证（可溯源）".to_string());
        lines.push("| # | 类型 | 声明内容 | 置信度 |".to_string());
        lines.push("|---|------|----------|--------|".to_string());
        for (i, r) in verified.iter().enumerate() {
            lines.push(format!(
                "| {} | {} | {} | {} |",
                i + 1,
                kind_label(r.kind),
                r.claim,
                confidence_label(r.confidence)
            ));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// 照 `validator.py:360-408`：自动修复日志 + 统计行 + 三张表。
fn format_validation_report(outcome: &ValidationOutcome) -> String {
    let errors: Vec<&Issue> = outcome
        .issues
        .iter()
        .filter(|i| i.level == "error")
        .collect();
    let warnings: Vec<&Issue> = outcome
        .issues
        .iter()
        .filter(|i| i.level == "warning")
        .collect();
    let infos: Vec<&Issue> = outcome.issues.iter().filter(|i| i.level == "info").collect();

    let mut lines = vec!["## [质量自检报告]".to_string(), String::new()];

    if !outcome.fix_log.is_empty() {
        lines.push(format!("自动修复 {} 项：", outcome.fix_log.len()));
        for f in &outcome.fix_log {
            for c in &f.changes {
                lines.push(format!("  - [{}] {}", f.task_index, c));
            }
        }
        lines.push(String::new());
    }

    lines.push(format!(
        "- [错误] {} 项 | [警告] {} 项 | [建议] {} 项",
        errors.len(),
        warnings.len(),
        infos.len()
    ));
    lines.push(format!(
        "- **总体: {}**",
        if outcome.pass {
            "[通过]"
        } else {
            "[未通过]（请修复错误后重新输出）"
        }
    ));
    lines.push(String::new());

    push_issue_table(&mut lines, "### 必须修复", "问题", &errors);
    push_issue_table(&mut lines, "### [请确认]", "问题", &warnings);
    push_issue_table(&mut lines, "### [优化建议]", "建议", &infos);

    lines.join("\n")
}

fn push_issue_table(lines: &mut Vec<String>, heading: &str, last_col: &str, issues: &[&Issue]) {
    if issues.is_empty() {
        return;
    }
    lines.push(heading.to_string());
    lines.push(format!("| # | 行动项 | 字段 | {last_col} |"));
    lines.push("|---|--------|------|------|".to_string());
    for (i, issue) in issues.iter().enumerate() {
        lines.push(format!(
            "| {} | {} | {} | {} |",
            i + 1,
            issue.task_name,
            issue.field,
            issue.message
        ));
    }
    lines.push(String::new());
}

fn kind_label(kind: ClaimKind) -> &'static str {
    match kind {
        ClaimKind::Decision => "决策",
        ClaimKind::Task => "行动项",
        ClaimKind::Risk => "风险",
    }
}

fn confidence_label(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::High => "高",
        Confidence::Medium => "中",
        Confidence::Low => "低",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minutes::llm::TaskItem;

    const SRC: &str = "会上确认华硕不落总灯已经结案了。冯亚的进度，龙哥这边帮忙跟进一下。预算还没批，可能来不及。";

    fn empty_validation() -> ValidationOutcome {
        ValidationOutcome::default()
    }

    fn verified_claim() -> ClaimResult {
        crate::minutes::traceability::verify_claim("华硕不落总灯已经结案", SRC, ClaimKind::Decision)
    }

    fn unsupported_claim() -> ClaimResult {
        crate::minutes::traceability::verify_claim(
            "采购部同意追加三百万元预算",
            SRC,
            ClaimKind::Decision,
        )
    }

    #[test]
    fn report_has_four_sections_in_order() {
        let md = render_quality_report(&[], &empty_validation(), &[]);
        for section in ["# 质检报告", "## 溯源校验", "## 质量自检", "## 任务冲突检测"] {
            assert!(md.contains(section), "缺少 {section}");
        }
    }

    #[test]
    fn empty_inputs_render_friendly_placeholders() {
        let md = render_quality_report(&[], &empty_validation(), &[]);
        assert!(md.contains("无需要校验的声明项"));
        assert!(md.contains("未发现任务冲突"));
    }

    #[test]
    fn stats_line_counts_verdicts() {
        let md = render_quality_report(
            &[verified_claim(), unsupported_claim()],
            &empty_validation(),
            &[],
        );
        assert!(md.contains("已验证: 1"));
        assert!(md.contains("无依据: 1"));
    }

    #[test]
    fn tables_match_python_columns() {
        let inferred = ClaimResult {
            claim: "预算可能来不及".to_string(),
            kind: ClaimKind::Risk,
            verdict: Verdict::Inferred,
            confidence: Confidence::Low,
            sources: vec!["预算还没批，可能来不及".to_string()],
            reason: "原文含部分匹配子串\"预算\"（4字），但整体属于推论".to_string(),
        };
        let validation = ValidationOutcome {
            fixed_tasks: vec![TaskItem::default()],
            issues: vec![Issue {
                task_index: "1".to_string(),
                task_name: "跟进".to_string(),
                level: "error",
                field: "责任人",
                message: "责任人不能为空".to_string(),
            }],
            pass: false,
            fix_log: vec![crate::minutes::validate::FixLog {
                task_index: 1,
                task_name: "跟进".to_string(),
                changes: vec!["日期: \"本周五\" → \"2026-08-07\"".to_string()],
            }],
        };
        let conflicts = vec![Conflict {
            kind: "same_person_same_deadline",
            severity: "high",
            description: "\"许日政\"在 2026-08-07 同时有 2 个任务待完成".to_string(),
            tasks: vec!["跟进A".to_string(), "跟进B".to_string()],
        }];

        let md = render_quality_report(
            &[verified_claim(), inferred, unsupported_claim()],
            &validation,
            &conflicts,
        );

        // 溯源三张表：列数 4 / 5 / 4
        assert!(md.contains("| # | 类型 | 声明内容 | 判定理由 |"));
        assert!(md.contains("| # | 类型 | 声明内容 | 置信度 | 原文依据 |"));
        assert!(md.contains("| # | 类型 | 声明内容 | 置信度 |"));
        assert!(md.contains("| 1 | 决策 | 华硕不落总灯已经结案 | 中 |"));
        assert!(md.contains("| 1 | 风险 | 预算可能来不及 | 低 | 预算还没批，可能来不及... |"));
        assert!(md.contains("| 1 | 决策 | 采购部同意追加三百万元预算 | 原文中未找到"));

        // 质检三张表（只有 error 一类）
        assert!(md.contains("## [质量自检报告]"));
        assert!(md.contains("自动修复 1 项："));
        assert!(md.contains("  - [1] 日期: \"本周五\" → \"2026-08-07\""));
        assert!(md.contains("- [错误] 1 项 | [警告] 0 项 | [建议] 0 项"));
        assert!(md.contains("- **总体: [未通过]（请修复错误后重新输出）**"));
        assert!(md.contains("| # | 行动项 | 字段 | 问题 |"));
        assert!(md.contains("| 1 | 跟进 | 责任人 | 责任人不能为空 |"));

        // 冲突表
        assert!(md.contains("共发现 1 处冲突（高: 1 | 中: 0）"));
        assert!(md.contains("### [高优先级冲突]"));
        assert!(md.contains("| # | 问题描述 | 涉及任务 |"));
        assert!(md.contains("跟进A、跟进B"));
    }
}
