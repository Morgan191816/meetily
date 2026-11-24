//! 会议纪要编排 — 移植自 `meeting_transcribe/assistant_bridge.py:583-725`。
//!
//! 顺序：术语纠正 → step2 → step3 → 冲突 → 参会人 → 溯源 → 质检 → 渲染 05/07 → Excel。
//! LLM 通过 `MinutesLlm` 注入（生产走统一模型配置，测试走 stub，不访问网络）。
//!
//! 与 Python 的差异（已裁定）：prompt 喂纠错后文本；日期先归一化再落 frontmatter。

use chrono::NaiveDate;
use tokio_util::sync::CancellationToken;

use crate::audio::transcription::term_correction::correct_terms;
use crate::minutes::conflict::detect_conflicts;
use crate::minutes::dates::parse_date;
use crate::minutes::excel::build_workbook_bytes;
use crate::minutes::llm::{MinutesDraft, TaskItem};
use crate::minutes::names::extract_attendees;
use crate::minutes::quality::render_quality_report;
use crate::minutes::render::{render_minutes, MinutesRenderInput};
use crate::minutes::traceability::{verify_claims, ClaimKind, ClaimResult, Verdict};
use crate::minutes::validate::validate_tasks;

/// 两次 LLM 调用；测试注入 stub，生产注入 reqwest 实现。
#[async_trait::async_trait]
pub trait MinutesLlm: Send + Sync {
    async fn summarize(&self, corrected_text: &str) -> Result<MinutesDraft, String>;
    async fn extract_tasks(&self, corrected_text: &str) -> Result<Vec<TaskItem>, String>;
}

/// 编排产物；`tasks` 为质检修复后的任务，用于落库与 Excel 导出。
#[derive(Debug)]
pub struct MinutesOutput {
    pub title: String,
    pub meeting_date: String,
    pub minutes_markdown: String,
    pub quality_markdown: String,
    pub excel_bytes: Vec<u8>,
    pub tasks: Vec<TaskItem>,
    pub traceability_pass: bool,
    pub validation_pass: bool,
}

/// 编排输入。`title_hint` / `project` / `source` / `metadata_created_at` 由命令层提供，
/// 测试可全部省略（brief 的 `run_minutes` 签名）。
pub struct MinutesInput<'a> {
    pub text: &'a str,
    pub members: &'a [String],
    pub today: NaiveDate,
    pub title_hint: Option<&'a str>,
    pub project: Option<&'a str>,
    pub source: Option<&'a str>,
    /// metadata.json 的 created_at（RFC3339），对应 Python `_fallback_date`。
    pub metadata_created_at: Option<&'a str>,
    pub cancel: Option<&'a CancellationToken>,
}

/// brief 的固定签名：不注入标题/项目/元数据，日期回退到 `today`。
pub async fn run_minutes(
    text: &str,
    llm: &dyn MinutesLlm,
    members: &[String],
    today: NaiveDate,
) -> Result<MinutesOutput, String> {
    run_minutes_with(
        MinutesInput {
            text,
            members,
            today,
            title_hint: None,
            project: None,
            source: None,
            metadata_created_at: None,
            cancel: None,
        },
        llm,
    )
    .await
}

pub async fn run_minutes_with(
    input: MinutesInput<'_>,
    llm: &dyn MinutesLlm,
) -> Result<MinutesOutput, String> {
    // 1. 术语纠正（单趟最长匹配；未初始化时原样返回）
    let corrected = correct_terms(input.text);
    ensure_not_cancelled(input.cancel)?;

    // 2. step2 总结
    let draft = llm
        .summarize(&corrected)
        .await
        .map_err(|error| format!("纪要生成失败：{error}"))?;
    ensure_not_cancelled(input.cancel)?;

    // 3. step3 任务拆解
    let tasks = llm
        .extract_tasks(&corrected)
        .await
        .map_err(|error| format!("行动项提取失败：{error}"))?;
    ensure_not_cancelled(input.cancel)?;

    // 4. 冲突检测（对原始任务，与 Python `_step35` 一致）
    let conflicts = detect_conflicts(&tasks);

    // 5. 参会人：先看「参会人员：」行，无则回退到说话人标签
    let (mut attendees, mut unmatched_attendees) = extract_attendees(&corrected, input.members);
    if attendees.is_empty() {
        let labels = speaker_labels(&corrected);
        if !labels.is_empty() {
            attendees = labels
                .iter()
                .filter(|name| input.members.iter().any(|member| member == *name))
                .cloned()
                .collect();
            unmatched_attendees = labels
                .into_iter()
                .filter(|name| !input.members.iter().any(|member| member == name))
                .collect();
        }
    }

    // 6. 溯源：按 kind 三次调用后合并（决策 → 风险 → 任务），不含议题标题
    let claims = build_claims(&draft, &tasks, &corrected);
    let traceability_pass = claims
        .iter()
        .all(|claim| !matches!(claim.verdict, Verdict::Unsupported));
    ensure_not_cancelled(input.cancel)?;

    // 7. 质检（自动修复 + 校验）
    let validation = validate_tasks(&tasks, input.members, input.today);

    // 8. 渲染 05 / 07
    let title = input
        .title_hint
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .or_else(|| {
            let draft_title = draft.title.trim();
            (!draft_title.is_empty()).then(|| draft_title.to_string())
        })
        .unwrap_or_else(|| "会议纪要".to_string());
    let meeting_date = resolve_meeting_date(&draft, input.metadata_created_at, input.today);
    let minutes = render_minutes(&MinutesRenderInput {
        title: title.clone(),
        meeting_date: meeting_date.clone(),
        project: input.project.unwrap_or("会议").to_string(),
        source: input.source.unwrap_or("逐字稿").to_string(),
        today: input.today,
        attendees,
        unmatched_attendees,
        draft: &draft,
        validation: &validation,
        claims: &claims,
        traceability_pass,
    });
    let quality_markdown = render_quality_report(&claims, &validation, &conflicts);

    // 9. Excel 字节
    let excel_bytes = build_workbook_bytes(&validation.fixed_tasks, input.today)?;

    Ok(MinutesOutput {
        title,
        meeting_date,
        minutes_markdown: minutes.markdown,
        quality_markdown,
        excel_bytes,
        tasks: validation.fixed_tasks,
        traceability_pass,
        validation_pass: validation.pass,
    })
}

/// S1：决策内容 → 风险描述 → 任务名，各自单 kind 校验后按顺序合并。
fn build_claims(draft: &MinutesDraft, tasks: &[TaskItem], source: &str) -> Vec<ClaimResult> {
    let decisions: Vec<String> = draft
        .decisions
        .iter()
        .map(|decision| decision.content.clone())
        .collect();
    let risks: Vec<String> = draft
        .risks
        .iter()
        .map(|risk| risk.description.clone())
        .collect();
    let task_names: Vec<String> = tasks
        .iter()
        .map(|task| task.task_name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();

    let mut claims = verify_claims(&decisions, source, ClaimKind::Decision);
    claims.extend(verify_claims(&risks, source, ClaimKind::Risk));
    claims.extend(verify_claims(&task_names, source, ClaimKind::Task));
    claims
}

/// S2：draft.date → metadata.created_at[:10] → today；可解析时归一化为 YYYY-MM-DD。
fn resolve_meeting_date(
    draft: &MinutesDraft,
    metadata_created_at: Option<&str>,
    today: NaiveDate,
) -> String {
    let raw = draft.date.trim();
    let raw = if !raw.is_empty() {
        raw.to_string()
    } else if let Some(created) = metadata_created_at
        .map(str::trim)
        .filter(|created| !created.is_empty())
    {
        created.chars().take(10).collect()
    } else {
        return today.format("%Y-%m-%d").to_string();
    };

    parse_date(&raw, today)
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or(raw)
}

/// 说话人标签：`### 张三 [00:01]` → 张三；跳过 SPEAKER_xx（照 `_extract_speaker_names`）。
/// 保留首次出现顺序（Python 用 list，不排序）。
fn speaker_labels(text: &str) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("### ") else {
            continue;
        };
        let label = rest.split('[').next().unwrap_or("").trim();
        if !label.is_empty()
            && !label.starts_with("SPEAKER_")
            && !labels.iter().any(|existing| existing == label)
        {
            labels.push(label.to_string());
        }
    }
    labels
}

fn ensure_not_cancelled(cancel: Option<&CancellationToken>) -> Result<(), String> {
    match cancel {
        Some(token) if token.is_cancelled() => Err("纪要生成已取消".to_string()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minutes::llm::Decision;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 4).unwrap()
    }

    fn members() -> Vec<String> {
        vec!["许日政".to_string(), "杨朋".to_string()]
    }

    fn step2_fixture() -> MinutesDraft {
        MinutesDraft {
            title: "路灯会议".to_string(),
            date: "2026-08-04".to_string(),
            decisions: vec![Decision {
                content: "华硕不落总灯已经结案".to_string(),
                basis: "会上确认".to_string(),
                effective_date: "2026-08-04".to_string(),
            }],
            ..Default::default()
        }
    }

    fn step3_fixture() -> Vec<TaskItem> {
        vec![TaskItem {
            task_name: "跟进冯亚项目进度".to_string(),
            owner: "许日政".to_string(),
            participants: "杨朋".to_string(),
            deadline: "2026-08-07".to_string(),
            ..Default::default()
        }]
    }

    struct StubLlm {
        draft: MinutesDraft,
        tasks: Vec<TaskItem>,
    }

    impl StubLlm {
        fn new(draft: MinutesDraft, tasks: Vec<TaskItem>) -> Self {
            Self { draft, tasks }
        }
    }

    #[async_trait::async_trait]
    impl MinutesLlm for StubLlm {
        async fn summarize(&self, _corrected_text: &str) -> Result<MinutesDraft, String> {
            Ok(self.draft.clone())
        }

        async fn extract_tasks(&self, _corrected_text: &str) -> Result<Vec<TaskItem>, String> {
            Ok(self.tasks.clone())
        }
    }

    struct FailingLlm;

    #[async_trait::async_trait]
    impl MinutesLlm for FailingLlm {
        async fn summarize(&self, _corrected_text: &str) -> Result<MinutesDraft, String> {
            Err("模型不可用".to_string())
        }

        async fn extract_tasks(&self, _corrected_text: &str) -> Result<Vec<TaskItem>, String> {
            Err("模型不可用".to_string())
        }
    }

    #[tokio::test]
    async fn orchestrator_produces_all_three_outputs() {
        let stub = StubLlm::new(step2_fixture(), step3_fixture());
        let out = run_minutes(
            "参会人员：许日政\n会上确认华硕不落总灯已经结案。跟进冯亚项目进度。",
            &stub,
            &members(),
            today(),
        )
        .await
        .unwrap();
        assert!(out.minutes_markdown.contains("## 决策事项"));
        assert!(out.quality_markdown.contains("## 溯源校验"));
        assert!(!out.excel_bytes.is_empty());
        assert_eq!(out.title, "路灯会议");
        assert_eq!(out.meeting_date, "2026-08-04");
        assert_eq!(out.tasks.len(), 1);
    }

    #[tokio::test]
    async fn llm_failure_keeps_error_readable() {
        let err = run_minutes("原文", &FailingLlm, &[], today())
            .await
            .unwrap_err();
        assert!(err.contains("纪要生成失败"));
    }

    #[test]
    fn claims_merge_decisions_risks_tasks_without_topics() {
        let draft = MinutesDraft {
            decisions: vec![Decision {
                content: "华硕不落总灯已经结案".to_string(),
                ..Default::default()
            }],
            topics: vec![crate::minutes::llm::Topic {
                title: "议题标题不应参与溯源".to_string(),
                ..Default::default()
            }],
            risks: vec![crate::minutes::llm::Risk {
                description: "预算还没批".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let claims = build_claims(&draft, &step3_fixture(), "华硕不落总灯已经结案。预算还没批。跟进冯亚项目进度。");
        let kinds: Vec<ClaimKind> = claims.iter().map(|claim| claim.kind).collect();
        assert_eq!(
            kinds,
            vec![ClaimKind::Decision, ClaimKind::Risk, ClaimKind::Task]
        );
        assert!(!claims.iter().any(|claim| claim.claim.contains("议题标题")));
    }

    #[test]
    fn meeting_date_falls_back_to_metadata_then_today() {
        let empty = MinutesDraft::default();
        assert_eq!(
            resolve_meeting_date(&empty, Some("2026-08-03T10:00:00+08:00"), today()),
            "2026-08-03"
        );
        assert_eq!(resolve_meeting_date(&empty, None, today()), "2026-08-04");

        let relative = MinutesDraft {
            date: "下周三".to_string(),
            ..Default::default()
        };
        assert_eq!(resolve_meeting_date(&relative, None, today()), "2026-08-12");
    }

    #[test]
    fn speaker_labels_skip_anonymous_speakers() {
        let labels = speaker_labels("### 张三 [00:01]\n内容\n### SPEAKER_00 [00:02]\n内容");
        assert_eq!(labels, vec!["张三"]);
    }
}
