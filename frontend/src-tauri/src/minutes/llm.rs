//! LLM 两步（step2 总结 / step3 任务拆解）— 移植自
//! `meeting_transcribe/assistant/tools/orchestrator.py` 的 prompt 与
//! `meeting_transcribe/assistant_bridge.py` 的 JSON 归一化逻辑。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rich_step2_schema() {
        let raw = r#"{"title":"路灯会议","date":"2026-08-04","decisions":[{"content":"结案","basis":"会上确认","effective_date":""}],"topics":[],"risks":[],"open_issues":[],"next_meeting":null}"#;
        let d = parse_step2_json(raw).unwrap();
        assert_eq!(d.title, "路灯会议");
        assert_eq!(d.decisions[0].content, "结案");
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
    fn parses_task_array_and_wrapped_object() {
        let arr = r#"[{"task_name":"跟进进度","责任人":"许日政","参与人":"杨朋","截止日期":"本周五"}]"#;
        assert_eq!(parse_step3_json(arr).len(), 1);
        let wrapped = r#"{"tasks":[{"task_name":"A"}]}"#;
        assert_eq!(parse_step3_json(wrapped)[0].task_name, "A");
    }
}
