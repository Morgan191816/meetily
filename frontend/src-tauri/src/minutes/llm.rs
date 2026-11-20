//! LLM 两步（step2 总结 / step3 任务拆解）— 移植自
//! `meeting_transcribe/assistant/tools/orchestrator.py:309-351` 的 prompt 与
//! `meeting_transcribe/assistant_bridge.py:259-325` 的 JSON 归一化逻辑。
//!
//! 输入使用术语纠正后的文本（已裁定偏离：Python 版在 step1 之前构造 prompt，
//! 实际喂的是未纠错原文）。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::summary::llm_client::{generate_summary, LLMProvider};
use crate::summary::CustomOpenAIConfig;

const SYSTEM_PROMPT: &str = "你是一个专业的会议记录助手。";

/// 照 assistant_bridge.py:259-269 的富结构 schema。
const STEP2_JSON_SCHEMA: &str = r#"{"title": "会议标题", "date": "YYYY-MM-DD", "decisions": [{"content": "决策内容", "basis": "决策依据", "effective_date": "生效时间或空"}], "topics": [{"title": "议题名", "discussion": "讨论要点", "conclusion": "结论", "disagreement": "分歧点或空"}], "risks": [{"description": "风险描述", "severity": "高/中/低", "mitigation": "缓解措施", "owner": "负责人"}], "open_issues": ["待决问题"], "next_meeting": {"date": "下次会议日期或空", "agenda": "议程"}}"#;

/// reasoning 模型在 JSON 前输出的思考标记（照 qwen_client.py:81-86）。
const REASONING_MARKERS: &[&str] = &[
    "Here's a thinking process:",
    "Here is the thinking process:",
    "Thinking process:",
    "Let me think about this",
    "好的，我来分析一下",
    "我来处理这个任务",
    "让我思考一下",
    "首先",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Decision {
    pub content: String,
    pub basis: String,
    pub effective_date: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Topic {
    pub title: String,
    pub discussion: String,
    pub conclusion: String,
    pub disagreement: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Risk {
    pub description: String,
    pub severity: String,
    pub mitigation: String,
    pub owner: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NextMeeting {
    pub date: String,
    pub agenda: String,
}

/// step2 的富结构；旧模型返回扁平字符串时由 `parse_step2_json` 升为对象。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MinutesDraft {
    pub title: String,
    pub date: String,
    pub decisions: Vec<Decision>,
    pub topics: Vec<Topic>,
    pub risks: Vec<Risk>,
    pub open_issues: Vec<String>,
    pub next_meeting: Option<NextMeeting>,
}

/// step3 的单条行动项。中英文键都接受（模型常按 prompt 返回中文键）。
#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TaskItem {
    #[serde(alias = "任务名称", alias = "taskName")]
    pub task_name: String,
    #[serde(alias = "责任人")]
    pub owner: String,
    #[serde(alias = "参与人")]
    pub participants: String,
    #[serde(alias = "截止日期", alias = "结束日期")]
    pub deadline: String,
    #[serde(alias = "描述")]
    pub description: String,
    #[serde(alias = "风险点")]
    pub risk: String,
    #[serde(alias = "难度点")]
    pub difficulty: String,
    #[serde(alias = "需协助点", alias = "help")]
    pub help_needed: String,
}

/// LLM 调用的统一模型配置（URL / Key / 协议 / 模型）。
pub struct LlmParams<'a> {
    pub client: &'a reqwest::Client,
    pub config: &'a CustomOpenAIConfig,
}

async fn call_llm(params: &LlmParams<'_>, user_prompt: &str) -> Result<String, String> {
    let completion = generate_summary(
        params.client,
        &LLMProvider::CustomOpenAI,
        &params.config.model,
        params.config.api_key.as_deref().unwrap_or(""),
        SYSTEM_PROMPT,
        user_prompt,
        None,
        Some(params.config.endpoint.as_str()),
        params.config.protocol,
        Some(8192),
        Some(0.3),
        None,
        None,
    )
    .await
    .map_err(|error| format!("调用大模型失败：{error}"))?;
    // content 为 null 时 completion 是空串，不能当成成功
    if completion.content.trim().is_empty() {
        return Err("大模型返回内容为空，请检查模型设置或稍后重试".to_string());
    }
    Ok(completion.content)
}

/// step2：会议总结。Python 版解析失败按空结构处理，这里返回可读错误。
pub async fn summarize(
    params: &LlmParams<'_>,
    corrected_text: &str,
) -> Result<MinutesDraft, String> {
    let raw = call_llm(params, &step2_prompt(corrected_text)).await?;
    parse_step2_json(&raw).ok_or_else(|| "纪要总结返回内容无法解析为 JSON".to_string())
}

/// step3：任务拆解。解析失败重试一次（照 assistant_bridge.py:314-325）。
pub async fn extract_tasks(
    params: &LlmParams<'_>,
    corrected_text: &str,
) -> Result<Vec<TaskItem>, String> {
    let prompt = step3_prompt(corrected_text);
    let raw = call_llm(params, &prompt).await?;
    if let Some(value) = extract_json(&raw, false) {
        return Ok(tasks_from_value(&value));
    }
    let raw = call_llm(params, &prompt).await?;
    Ok(parse_step3_json(&raw))
}

fn step2_prompt(corrected_text: &str) -> String {
    format!(
        "请根据以下会议记录原文，提取会议信息：\n\n\
         ## 原始文本\n{corrected_text}\n\n\
         请输出：\n\
         1. **会议标题**（10-20字概括核心目的）\n\
         2. **会议日期**（YYYY-MM-DD 格式）\n\
         3. **核心决策**（最重要的1-3条，每条一句话）\n\
         4. **详细议题**（如有，每个议题一句话概括）\n\
         5. **风险与阻碍**（如有）\n\n\
         要求：\n\
         - 每条决策/议题必须能在原文中找到依据\n\
         - 推论/总结请标注\"[推论]\"\n\
         - 无依据的内容不要输出\n\n\
         请严格以 JSON 输出（不要输出其他文字），每条摘要 ≤30 字，无则空数组：{STEP2_JSON_SCHEMA}"
    )
}

fn step3_prompt(corrected_text: &str) -> String {
    const EXAMPLE: &str = r#"[
  {
    "task_name": "动词开头的任务描述",
    "责任人": "唯一责任人姓名",
    "参与人": "参与人姓名（多人用英文逗号分隔）",
    "截止日期": "保持原始格式"
  }
]"#;
    format!(
        "请从以下会议记录中提取所有行动项（action items）：\n\n\
         ## 原始文本\n{corrected_text}\n\n\
         请输出 JSON 格式的行动项列表：\n```json\n{EXAMPLE}\n```\n\n\
         要求：\n\
         - 每项任务只有一个主责任人\n\
         - 任务名称以动词开头（评估/确认/完成/启动/提交等）\n\
         - 截止日期保持原文格式（后续自动转换）\n\
         - 如无行动项，输出空列表"
    )
}

/// 解析 step2 富结构；无法提取 JSON 对象时返回 None。
pub fn parse_step2_json(raw: &str) -> Option<MinutesDraft> {
    let value = extract_json(raw, true)?;
    let obj = value.as_object()?;

    Some(MinutesDraft {
        title: first_str(obj, &["title", "会议标题"]),
        date: first_str(obj, &["date", "会议日期"]),
        decisions: norm_items(
            obj,
            &["decisions", "核心决策"],
            "content",
            &["basis", "effective_date"],
        )
        .into_iter()
        .map(|item| Decision {
            content: map_str(&item, "content"),
            basis: map_str(&item, "basis"),
            effective_date: map_str(&item, "effective_date"),
        })
        .collect(),
        topics: norm_items(
            obj,
            &["topics", "详细议题"],
            "title",
            &["discussion", "conclusion", "disagreement"],
        )
        .into_iter()
        .map(|item| Topic {
            title: map_str(&item, "title"),
            discussion: map_str(&item, "discussion"),
            conclusion: map_str(&item, "conclusion"),
            disagreement: map_str(&item, "disagreement"),
        })
        .collect(),
        risks: norm_items(
            obj,
            &["risks", "风险与阻碍"],
            "description",
            &["severity", "mitigation", "owner"],
        )
        .into_iter()
        .map(|item| Risk {
            description: map_str(&item, "description"),
            severity: map_str(&item, "severity"),
            mitigation: map_str(&item, "mitigation"),
            owner: map_str(&item, "owner"),
        })
        .collect(),
        open_issues: first_array(obj, &["open_issues", "待决问题", "待决事项"])
            .into_iter()
            .flatten()
            .map(|item| value_text(Some(item)))
            .filter(|text| !text.is_empty())
            .collect(),
        next_meeting: parse_next_meeting(obj),
    })
}

/// 解析 step3；接受数组或 `{tasks|行动项}` 包装对象，脏元素跳过而非整体失败。
pub fn parse_step3_json(raw: &str) -> Vec<TaskItem> {
    extract_json(raw, false)
        .map(|value| tasks_from_value(&value))
        .unwrap_or_default()
}

fn tasks_from_value(value: &Value) -> Vec<TaskItem> {
    let items: &[Value] = match value {
        Value::Array(items) => items,
        // 照 Python 的 a or b：tasks 为 null/空数组时回退到行动项
        Value::Object(obj) => first_array(obj, &["tasks", "行动项"])
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        _ => &[],
    };
    items
        .iter()
        .filter_map(|item| serde_json::from_value(item.clone()).ok())
        .collect()
}

/// 剥思考前缀、剥代码围栏、整体解析失败时扫描所有 `{`/`[` 起点，
/// 取结束位置最靠后的合法 JSON（照 qwen_client.py:104-156）。
fn extract_json(raw: &str, want_object: bool) -> Option<Value> {
    if raw.trim().is_empty() {
        return None;
    }
    let content = strip_code_fence(strip_reasoning_preamble(raw));

    if let Ok(value) = serde_json::from_str::<Value>(&content) {
        if !want_object || value.is_object() {
            return Some(value);
        }
    }

    let mut best: Option<(usize, Value)> = None;
    for (start, _) in content.match_indices(|c: char| c == '{' || c == '[') {
        let mut stream = serde_json::Deserializer::from_str(&content[start..]).into_iter::<Value>();
        if let Some(Ok(value)) = stream.next() {
            if want_object && !value.is_object() {
                continue;
            }
            let end = start + stream.byte_offset();
            if best.as_ref().is_none_or(|(best_end, _)| end > *best_end) {
                best = Some((end, value));
            }
        }
    }
    best.map(|(_, value)| value)
}

fn strip_reasoning_preamble(content: &str) -> &str {
    let json_start = content.find(['{', '[']);
    for marker in REASONING_MARKERS {
        let Some(idx) = content.find(marker) else {
            continue;
        };
        // 标记出现在 JSON 内部（如 "content":"首先确认…"）时不得截断
        if json_start.is_none_or(|start| idx >= start) {
            continue;
        }
        let after_marker = &content[idx + marker.len()..];
        if let Some(offset) = after_marker.find(['{', '[']) {
            return &after_marker[offset..];
        }
    }
    content
}

fn strip_code_fence(content: &str) -> String {
    let fenced = content
        .split("```json")
        .nth(1)
        .or_else(|| content.split("```").nth(1));
    match fenced {
        Some(rest) => rest
            .split("```")
            .next()
            .unwrap_or(rest)
            .trim()
            .to_string(),
        None => content.to_string(),
    }
}

/// Python 用 `a or b` 取值，空数组/空串会回退到中文键。
fn first_array<'a>(obj: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Vec<Value>> {
    keys.iter()
        .filter_map(|key| obj.get(*key))
        .find_map(|value| value.as_array().filter(|items| !items.is_empty()))
}

fn first_str(obj: &Map<String, Value>, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|key| obj.get(*key))
        .map(|value| value_text(Some(value)))
        .find(|text| !text.is_empty())
        .unwrap_or_default()
}

fn parse_next_meeting(obj: &Map<String, Value>) -> Option<NextMeeting> {
    let meeting = ["next_meeting", "下次会议"]
        .iter()
        .filter_map(|key| obj.get(*key))
        .find_map(|value| value.as_object().filter(|map| !map.is_empty()))?;
    let date = first_str(meeting, &["date", "日期"]);
    // agenda 为数组时用「；」拼接（照 assistant_bridge.py:482-484）
    let agenda = match meeting.get("agenda").or_else(|| meeting.get("议程")) {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| value_text(Some(item)))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("；"),
        other => value_text(other),
    };
    (!date.is_empty() || !agenda.is_empty()).then_some(NextMeeting { date, agenda })
}

/// 把扁平字符串列表或对象列表统一归一化为对象列表（照 _norm_str_list）。
fn norm_items(
    obj: &Map<String, Value>,
    keys: &[&str],
    primary: &str,
    extras: &[&str],
) -> Vec<Map<String, Value>> {
    let Some(items) = first_array(obj, keys) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let mut normalized = Map::new();
            match item {
                Value::Object(source) => {
                    normalized.insert(primary.to_string(), Value::String(map_str(source, primary)));
                    for extra in extras {
                        normalized.insert((*extra).to_string(), Value::String(map_str(source, extra)));
                    }
                }
                Value::Null => return None,
                other => {
                    normalized.insert(primary.to_string(), Value::String(value_text(Some(other))));
                    for extra in extras {
                        normalized.insert((*extra).to_string(), Value::String(String::new()));
                    }
                }
            }
            Some(normalized)
        })
        .collect()
}

fn map_str(map: &Map<String, Value>, key: &str) -> String {
    value_text(map.get(key))
}

fn value_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.trim().to_string(),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rich_step2_schema() {
        let raw = r#"{"title":"路灯会议","date":"2026-08-04","decisions":[{"content":"结案","basis":"会上确认","effective_date":""}],"topics":[],"risks":[],"open_issues":[],"next_meeting":null}"#;
        let d = parse_step2_json(raw).unwrap();
        assert_eq!(d.title, "路灯会议");
        assert_eq!(d.decisions[0].content, "结案");
        // agenda 为数组时用「；」拼接
        let agenda = parse_step2_json(r#"{"next_meeting":{"agenda":["预算","排期"]}}"#)
            .unwrap()
            .next_meeting
            .unwrap()
            .agenda;
        assert_eq!(agenda, "预算；排期");
    }

    #[test]
    fn accepts_legacy_chinese_keys() {
        let raw = r#"{"核心决策":["结案"],"详细议题":[{"title":"进度","conclusion":"继续"}],"风险与阻碍":["预算未批"]}"#;
        let d = parse_step2_json(raw).unwrap();
        assert_eq!(d.decisions[0].content, "结案");
        assert_eq!(d.topics[0].title, "进度");
        assert_eq!(d.risks[0].description, "预算未批");
    }

    #[test]
    fn tolerates_json_inside_code_fence_and_prose() {
        let raw = "先思考一下……\n```json\n{\"title\":\"A\"}\n```\n以上。";
        assert_eq!(parse_step2_json(raw).unwrap().title, "A");
    }

    #[test]
    fn returns_none_on_garbage() {
        assert!(parse_step2_json("不是 JSON").is_none());
    }

    #[test]
    fn strips_reasoning_preamble_before_json() {
        assert_eq!(
            parse_step2_json("首先\n{\"title\":\"A\"}").unwrap().title,
            "A"
        );
        assert_eq!(
            parse_step2_json("结论：{\"title\":\"A\"} 完毕")
                .unwrap()
                .title,
            "A"
        );
    }

    #[test]
    fn keeps_marker_inside_json() {
        // 标记出现在 JSON 内部时不得截断
        assert_eq!(
            parse_step2_json(r#"{"title":"A","decisions":[{"content":"首先确认方案"}]}"#)
                .unwrap()
                .decisions[0]
                .content,
            "首先确认方案"
        );
    }

    #[test]
    fn parses_task_array_and_wrapped_object() {
        let arr = r#"[{"task_name":"跟进进度","责任人":"许日政","参与人":"杨朋","截止日期":"本周五"}]"#;
        assert_eq!(parse_step3_json(arr).len(), 1);
        let wrapped = r#"{"tasks":[{"task_name":"A"}]}"#;
        assert_eq!(parse_step3_json(wrapped)[0].task_name, "A");
        // tasks 为 null 时回退到行动项；结束日期/help 也是合法 alias
        let fallback = parse_step3_json(
            r#"{"tasks":null,"行动项":[{"task_name":"B","结束日期":"下周一","help":"需设计支持"}]}"#,
        );
        assert_eq!(fallback[0].task_name, "B");
        assert_eq!(fallback[0].deadline, "下周一");
        assert_eq!(fallback[0].help_needed, "需设计支持");
    }
}
