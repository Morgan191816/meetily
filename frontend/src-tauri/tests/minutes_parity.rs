//! Phase 4 Task 9：与 Python 会议助手桥接层的 parity 测试。
//!
//! 同一份合成逐字稿 + 同一份 step2/step3 stub 响应，分别跑 Python 桥接层与 Rust
//! `run_minutes_with`，只比结构（区块标题与顺序、条数、内联标记、表格列名、07 统计行），
//! 不比 LLM 文本。黄金结构见 `tests/fixtures/minutes/expected_structure.json`。
//!
//! 另含 9 条已裁定行为偏离的用例（S2），每条断言 Rust 是修好的那个行为。
//! 全部不访问网络。

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Mutex, Once};

use calamine::{open_workbook_auto_from_rs, DataType, Reader};
use chrono::NaiveDate;
use serde::Deserialize;

use app_lib::audio::transcription::term_correction;
use app_lib::minutes::dates::parse_date;
use app_lib::minutes::llm::{parse_step2_json, parse_step3_json, MinutesDraft, TaskItem};
use app_lib::minutes::orchestrator::{run_minutes_with, MinutesInput, MinutesLlm, MinutesOutput};
use app_lib::minutes::quality::render_quality_report;
use app_lib::minutes::render::{render_minutes, MinutesRenderInput};
use app_lib::minutes::validate::{validate_tasks, ValidationOutcome};

const TODAY: (i32, u32, u32) = (2026, 8, 4); // 周二

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(TODAY.0, TODAY.1, TODAY.2).unwrap()
}

fn fixture(name: &str) -> String {
    format!(
        "{}/tests/fixtures/minutes/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn read_fixture(name: &str) -> String {
    std::fs::read_to_string(fixture(name))
        .unwrap_or_else(|error| panic!("读取夹具 {name} 失败: {error}"))
}

// ── Python 黄金结构 ──────────────────────────────────────

#[derive(Deserialize)]
struct Golden {
    minutes: GoldenMinutes,
    excel: GoldenExcel,
    quality: GoldenQuality,
}

#[derive(Deserialize)]
struct GoldenMinutes {
    frontmatter_keys: Vec<String>,
    h2_sections: Vec<String>,
    core_conclusion_count: usize,
    decision_count: usize,
    decision_markers: Vec<String>,
    action_table_header: String,
    action_row_count: usize,
    action_markers: Vec<String>,
    has_open_issues: bool,
    has_next_meeting: bool,
    has_evidence: bool,
}

#[derive(Deserialize)]
struct GoldenExcel {
    headers: Vec<String>,
    row_count: usize,
    data_row_count: usize,
    rows: Vec<Vec<String>>,
}

#[derive(Deserialize)]
struct GoldenQuality {
    h2_sections: Vec<String>,
    traceability_stats: Vec<String>,
    quality_stats: Vec<String>,
    has_pending_feedback_section: bool,
}

fn load_golden() -> Golden {
    serde_json::from_str(&read_fixture("expected_structure.json")).expect("黄金结构 JSON 应可解析")
}

// ── 结构解析辅助 ─────────────────────────────────────────

fn h2_sections(markdown: &str) -> Vec<String> {
    markdown
        .lines()
        .filter_map(|line| line.strip_prefix("## "))
        .map(str::to_string)
        .collect()
}

fn frontmatter_keys(markdown: &str) -> Vec<String> {
    let rest = markdown
        .strip_prefix("---\n")
        .expect("应以 YAML frontmatter 开头");
    rest.split("---\n")
        .next()
        .unwrap()
        .lines()
        .filter_map(|line| line.split_once(':').map(|(key, _)| key.to_string()))
        .collect()
}

fn marker_in(line: &str) -> String {
    ["已溯源", "推论", "待核实"]
        .into_iter()
        .find(|marker| line.contains(&format!("〔{marker}〕")))
        .unwrap_or("")
        .to_string()
}

fn decision_markers(markdown: &str) -> Vec<String> {
    markdown
        .lines()
        .filter(|line| line.starts_with("- 决策-"))
        .map(marker_in)
        .collect()
}

/// 行动项表的数据行（6 列 → split('|') 后 8 段；修订记录 3 列只有 5 段，自然排除）。
fn action_rows(markdown: &str) -> Vec<Vec<String>> {
    markdown
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            (cells.len() == 8 && cells[1].parse::<usize>().is_ok()).then(|| {
                cells[1..cells.len() - 1]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            })
        })
        .collect()
}

fn core_conclusion_count(markdown: &str) -> usize {
    let mut inside = false;
    let mut count = 0;
    for line in markdown.lines() {
        if line == "## 核心结论" {
            inside = true;
        } else if inside {
            if line.starts_with("## ") {
                break;
            }
            if line.starts_with("- ") {
                count += 1;
            }
        }
    }
    count
}

fn line_starting<'a>(markdown: &'a str, prefix: &str) -> &'a str {
    markdown
        .lines()
        .find(|line| line.starts_with(prefix))
        .unwrap_or_else(|| panic!("缺少以 {prefix} 开头的行"))
}

fn read_excel_rows(bytes: &[u8]) -> Vec<Vec<String>> {
    let mut workbook = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec())).unwrap();
    let range = workbook.worksheet_range("任务导入").unwrap();
    range
        .rows()
        .map(|row| {
            row.iter()
                .map(|cell| cell.as_string().unwrap_or_default())
                .collect()
        })
        .collect()
}

// ── stub LLM ─────────────────────────────────────────────

struct FixtureLlm {
    draft: MinutesDraft,
    tasks: Vec<TaskItem>,
    prompts: Mutex<Vec<String>>,
}

impl FixtureLlm {
    fn from_fixtures() -> Self {
        Self {
            draft: parse_step2_json(&read_fixture("step2_response.json")).unwrap(),
            tasks: parse_step3_json(&read_fixture("step3_response.json")),
            prompts: Mutex::new(Vec::new()),
        }
    }

    fn with_draft(draft: MinutesDraft) -> Self {
        Self {
            draft,
            tasks: Vec::new(),
            prompts: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl MinutesLlm for FixtureLlm {
    async fn summarize(
        &self,
        corrected_text: &str,
    ) -> Result<(MinutesDraft, Option<(u32, u32)>), String> {
        self.prompts
            .lock()
            .unwrap()
            .push(corrected_text.to_string());
        Ok((self.draft.clone(), None))
    }

    async fn extract_tasks(
        &self,
        corrected_text: &str,
    ) -> Result<(Vec<TaskItem>, Option<(u32, u32)>), String> {
        self.prompts
            .lock()
            .unwrap()
            .push(corrected_text.to_string());
        Ok((self.tasks.clone(), None))
    }
}

fn fixture_members() -> Vec<String> {
    let value: serde_json::Value =
        serde_json::from_str(&read_fixture("members.json")).expect("members.json");
    value["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member["name"].as_str().unwrap().to_string())
        .collect()
}

fn input<'a>(text: &'a str, members: &'a [String]) -> MinutesInput<'a> {
    MinutesInput {
        text,
        members,
        today: today(),
        title_hint: Some("测试会议"),
        project: Some("测试会议"),
        source: Some("01_原文.md"),
        metadata_created_at: None,
        cancel: None,
    }
}

async fn run_fixture() -> MinutesOutput {
    // 术语表是进程级 OnceLock；这里显式初始化，避免结果取决于 deviation_1/2 是否先跑
    init_terms();
    let llm = FixtureLlm::from_fixtures();
    let text = read_fixture("transcript_raw.txt");
    let members = fixture_members();
    run_minutes_with(input(&text, &members), &llm)
        .await
        .expect("编排应成功")
}

// ── parity：05 / 06 / 07 ────────────────────────────────

#[tokio::test]
async fn parity_minutes_structure_matches_python_golden() {
    let golden = load_golden();
    let out = run_fixture().await;
    let markdown = &out.minutes_markdown;

    assert_eq!(frontmatter_keys(markdown), golden.minutes.frontmatter_keys);
    assert_eq!(h2_sections(markdown), golden.minutes.h2_sections);
    assert_eq!(
        core_conclusion_count(markdown),
        golden.minutes.core_conclusion_count
    );

    // 编号两位零填充：改成「决策-1：」时这里必须红
    assert!(
        markdown.contains("- 决策-01："),
        "决策编号应为两位零填充"
    );

    let decisions = decision_markers(markdown);
    assert_eq!(decisions.len(), golden.minutes.decision_count);
    assert_eq!(decisions, golden.minutes.decision_markers);

    let header = markdown
        .lines()
        .find(|line| line.starts_with("| 序号 | 任务名称 |"))
        .expect("缺少行动项表头");
    assert_eq!(header, golden.minutes.action_table_header);

    let rows = action_rows(markdown);
    assert_eq!(rows.len(), golden.minutes.action_row_count);
    let markers: Vec<String> = rows.iter().map(|row| marker_in(&row[1])).collect();
    assert_eq!(markers, golden.minutes.action_markers);

    assert_eq!(
        markdown.contains("## 待决问题"),
        golden.minutes.has_open_issues
    );
    assert_eq!(
        markdown.contains("## 下次会议"),
        golden.minutes.has_next_meeting
    );
    assert_eq!(
        markdown.contains("## 溯源出处"),
        golden.minutes.has_evidence
    );

    // 与 07 统计行一致的闸门结论
    assert!(!out.traceability_pass, "3 条无依据 → 溯源不通过");
    assert!(out.validation_pass, "0 个 error → 质检通过");
}

#[tokio::test]
async fn parity_excel_structure_matches_python_golden() {
    let golden = load_golden();
    let out = run_fixture().await;
    let rows = read_excel_rows(&out.excel_bytes);

    assert_eq!(rows.len(), golden.excel.row_count);
    assert_eq!(rows[0], golden.excel.headers);
    assert_eq!(rows.len() - 1, golden.excel.data_row_count);
    for row in &rows {
        assert_eq!(row.len(), 12, "每行必须 12 列: {row:?}");
    }

    // 结束日期/紧急程度依赖参考日（Python 用系统日期，Rust 用注入日期），只比稳定列。
    for (rust_row, python_row) in rows[1..].iter().zip(golden.excel.rows.iter()) {
        assert_eq!(&rust_row[0..3], &python_row[0..3], "任务名/责任人/参与人");
        assert_eq!(rust_row[4], "", "上级任务必须为空");
        assert_eq!(rust_row[6], "", "开始日期必须为空");
        assert_eq!(rust_row[11], "", "开放下级任务必须为空");
        assert!(
            NaiveDate::parse_from_str(&rust_row[3], "%Y-%m-%d").is_ok(),
            "结束日期必须是 YYYY-MM-DD: {}",
            rust_row[3]
        );
        assert!(
            ["紧急", "高", "中", "低"].contains(&rust_row[5].as_str()),
            "紧急程度非法: {}",
            rust_row[5]
        );
    }
}

#[tokio::test]
async fn parity_quality_structure_matches_python_golden() {
    let golden = load_golden();
    let out = run_fixture().await;
    let report = &out.quality_markdown;

    assert!(
        golden.quality.has_pending_feedback_section,
        "Python 黄金结构应含「待确认沉淀候选」（Rust 已裁定移除）"
    );
    let mut expected_sections = golden.quality.h2_sections.clone();
    expected_sections.retain(|section| section != "待确认沉淀候选");
    assert_eq!(h2_sections(report), expected_sections);
    assert!(!report.contains("## 待确认沉淀候选"));

    assert_eq!(
        line_starting(report, "- ✅ 已验证:"),
        golden.quality.traceability_stats[0]
    );
    assert_eq!(
        line_starting(report, "- [错误]"),
        golden.quality.quality_stats[0]
    );
    assert!(report.contains("## 任务冲突检测"));
}

// ── 9 条已裁定行为偏离（S2）─────────────────────────────

fn init_terms() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        term_correction::init(Some(PathBuf::from(fixture("terms.json"))), None);
    });
}

/// 偏离 #1：术语替换用单趟最长匹配，已正确的「东软载波」不会被替换成「东软载波载波」。
#[tokio::test]
async fn deviation_1_term_correction_does_not_double_replace() {
    init_terms();
    let llm = FixtureLlm::with_draft(MinutesDraft::default());
    let members = fixture_members();
    run_minutes_with(input("东软载波方案和东软发布", &members), &llm)
        .await
        .unwrap();

    let prompts = llm.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 2, "step2 + step3 各一次");
    for prompt in &prompts {
        assert!(
            prompt.contains("东软载波方案和东软载波发布"),
            "应做单趟纠正: {prompt}"
        );
        assert!(!prompt.contains("东软载波载波"), "不应产生叠词: {prompt}");
    }
}

/// 偏离 #2：LLM 收到的是纠错后文本，而不是未纠错原文。
#[tokio::test]
async fn deviation_2_llm_receives_corrected_text() {
    init_terms();
    let llm = FixtureLlm::with_draft(MinutesDraft::default());
    let members = fixture_members();
    run_minutes_with(input("东软发布方案", &members), &llm)
        .await
        .unwrap();

    let prompts = llm.prompts.lock().unwrap().clone();
    assert!(!prompts.is_empty());
    for prompt in &prompts {
        assert!(
            prompt.contains("东软载波发布方案"),
            "应为纠错后文本: {prompt}"
        );
        assert!(
            !prompt.contains("东软发布方案"),
            "不应喂未纠错原文: {prompt}"
        );
    }
}

/// 偏离 #3：日期先归一化再进 frontmatter（Python 会把「下周三」原样写进去）。
#[tokio::test]
async fn deviation_3_date_normalized_before_frontmatter() {
    let draft = MinutesDraft {
        title: "日期测试".to_string(),
        date: "下周三".to_string(),
        ..Default::default()
    };
    let llm = FixtureLlm::with_draft(draft);
    let members = fixture_members();
    let out = run_minutes_with(input("原文", &members), &llm)
        .await
        .unwrap();

    assert_eq!(out.meeting_date, "2026-08-12");
    assert!(out.minutes_markdown.contains("date: 2026-08-12"));
    assert!(!out.minutes_markdown.contains("下周三"));
}

/// 偏离 #4：Python 的死代码不移植（注释不算，按非注释代码检查）。
#[test]
fn deviation_4_dead_code_not_ported() {
    let sources = [
        include_str!("../src/minutes/urgency.rs"),
        include_str!("../src/minutes/names.rs"),
        include_str!("../src/minutes/orchestrator.rs"),
        include_str!("../src/minutes/llm.rs"),
    ];
    let code: String = sources
        .iter()
        .flat_map(|source| source.lines())
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    for needle in [
        "HIGH_KEYWORDS",
        "MEDIUM_KEYWORDS",
        "pinyin",
        "_step5_output",
        "_extract_raw_text",
    ] {
        assert!(!code.contains(needle), "死代码不应移植: {needle}");
    }
}

/// 偏离 #5：07 不输出空的「待确认沉淀候选」节（Python 恒输出）。
#[test]
fn deviation_5_quality_report_omits_pending_feedback_section() {
    let report = render_quality_report(&[], &ValidationOutcome::default(), &[], None);
    assert!(!report.contains("待确认沉淀候选"));
    assert_eq!(
        h2_sections(&report),
        vec!["溯源校验", "质量自检", "任务冲突检测"]
    );
}

/// 偏离 #6：支持「本星期三」这类写法（Python 正则误写成字符类，匹配不到）。
#[test]
fn deviation_6_parses_ben_xingqisan() {
    assert_eq!(
        parse_date("本星期三", today()),
        Some(NaiveDate::from_ymd_opt(2026, 8, 5).unwrap())
    );
}

/// 偏离 #7：system prompt 只含角色句，不再下发 Python 的扁平 JSON 键名。
#[test]
fn deviation_7_system_prompt_is_role_only() {
    let line = include_str!("../src/minutes/llm.rs")
        .lines()
        .find(|line| line.trim_start().starts_with("const SYSTEM_PROMPT"))
        .expect("llm.rs 应定义 SYSTEM_PROMPT");
    assert!(line.contains("你是一个专业的会议记录助手。"));
    for forbidden in ["summary", "action_items", "decisions"] {
        assert!(
            !line.contains(forbidden),
            "system prompt 不应包含键名 {forbidden}: {line}"
        );
    }
}

/// 偏离 #8：`2026-02-30` 判 error 且 validation_pass=false（Python 原样放过）。
#[test]
fn deviation_8_invalid_date_is_error_and_fails_validation() {
    assert_eq!(parse_date("2026-02-30", today()), None);

    let task = TaskItem {
        task_name: "跟进进度".to_string(),
        owner: "许日政".to_string(),
        deadline: "2026-02-30".to_string(),
        ..Default::default()
    };
    let members = vec!["许日政".to_string()];
    let outcome = validate_tasks(&[task], &members, today());

    assert!(!outcome.pass);
    assert!(outcome
        .issues
        .iter()
        .any(|issue| issue.level == "error" && issue.field == "截止日期"));

    let report = render_quality_report(&[], &outcome, &[], None);
    assert!(report.contains("日期格式无法识别"));
    assert!(report.contains("[未通过]"));
}

/// 偏离 #9：`next_meeting` 只有额外键时省略整节（Python 会输出空的 `## 下次会议`）。
#[test]
fn deviation_9_next_meeting_with_only_extra_keys_is_omitted() {
    let draft =
        parse_step2_json(r#"{"title":"x","next_meeting":{"location":"会议室A","host":"张三"}}"#)
            .unwrap();
    assert!(draft.next_meeting.is_none());

    let validation = ValidationOutcome::default();
    let document = render_minutes(&MinutesRenderInput {
        title: "x".to_string(),
        meeting_date: "2026-08-04".to_string(),
        project: "会议".to_string(),
        source: "逐字稿".to_string(),
        today: today(),
        attendees: Vec::new(),
        unmatched_attendees: Vec::new(),
        draft: &draft,
        validation: &validation,
        claims: &[],
        traceability_pass: true,
    });
    assert!(!document.markdown.contains("## 下次会议"));
}
