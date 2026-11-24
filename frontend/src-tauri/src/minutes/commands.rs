//! 会议纪要命令 — Tauri 入口 + `meeting_minutes` 持久化。
//!
//! 对应 `assistant_bridge.py` 的文件落盘：05 纪要 / 07 质检 / 06 Excel。
//! 命令只接受 meeting_id；LLM 配置读 `customOpenAIConfig`（含 protocol）。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, Runtime};
use tokio_util::sync::CancellationToken;

use crate::database::repositories::meeting::MeetingsRepository;
use crate::database::repositories::setting::SettingsRepository;
use crate::minutes::excel::build_workbook_bytes;
use crate::minutes::llm::{MinutesDraft, TaskItem};
use crate::minutes::orchestrator::{run_minutes_with, MinutesInput, MinutesLlm};
use crate::state::AppState;
use crate::summary::CustomOpenAIConfig;

/// 在途纪要任务，`api_cancel_minutes` 按 meeting_id 取消。
static MINUTES_CANCELLATIONS: LazyLock<Mutex<HashMap<String, CancellationToken>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 生产 LLM：统一模型配置 + reqwest 客户端。
struct ReqwestMinutesLlm {
    client: reqwest::Client,
    config: CustomOpenAIConfig,
}

#[async_trait::async_trait]
impl MinutesLlm for ReqwestMinutesLlm {
    async fn summarize(&self, corrected_text: &str) -> Result<MinutesDraft, String> {
        crate::minutes::llm::summarize(
            &crate::minutes::llm::LlmParams {
                client: &self.client,
                config: &self.config,
            },
            corrected_text,
        )
        .await
    }

    async fn extract_tasks(&self, corrected_text: &str) -> Result<Vec<TaskItem>, String> {
        crate::minutes::llm::extract_tasks(
            &crate::minutes::llm::LlmParams {
                client: &self.client,
                config: &self.config,
            },
            corrected_text,
        )
        .await
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinutesRecord {
    pub meeting_id: String,
    pub markdown: String,
    pub quality_markdown: String,
    pub tasks: Vec<TaskItem>,
    pub traceability_pass: bool,
    pub validation_pass: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 生成会议纪要并写入 `meeting_minutes`。
#[tauri::command]
pub async fn api_generate_minutes<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<MinutesRecord, String> {
    let meeting_id = meeting_id.trim().to_string();
    if meeting_id.is_empty() {
        return Err("会议 ID 不能为空".to_string());
    }

    let pool = state.db_manager.pool();
    let meeting = MeetingsRepository::get_meeting_metadata(pool, &meeting_id)
        .await
        .map_err(|error| format!("加载会议元数据失败：{error}"))?
        .ok_or_else(|| format!("未找到会议：{meeting_id}"))?;

    let transcript = load_transcript_text(pool, &meeting_id).await?;
    let config = SettingsRepository::get_custom_openai_config(pool)
        .await
        .map_err(|error| format!("读取大模型配置失败：{error}"))?
        .ok_or_else(|| "尚未配置自定义大模型，无法生成纪要".to_string())?;

    let folder = meeting.folder_path.as_deref().map(Path::new);
    let metadata = folder.map(read_meeting_metadata).unwrap_or_default();
    let project = folder
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_string);
    let created_at = metadata
        .created_at
        .clone()
        .unwrap_or_else(|| meeting.created_at.0.to_rfc3339());
    let source = metadata.audio_file.clone();

    let llm = ReqwestMinutesLlm {
        client: reqwest::Client::new(),
        config,
    };
    let token = CancellationToken::new();
    register_cancellation(&meeting_id, token.clone());

    let result = run_minutes_with(
        MinutesInput {
            text: &transcript,
            members: &[],
            today: Utc::now().date_naive(),
            title_hint: Some(meeting.title.as_str()),
            project: project.as_deref(),
            source: source.as_deref(),
            metadata_created_at: Some(created_at.as_str()),
            cancel: Some(&token),
        },
        &llm,
    )
    .await;

    clear_cancellation(&meeting_id);
    let output = result?;

    let now = Utc::now().to_rfc3339();
    let tasks_json = serde_json::to_string(&output.tasks)
        .map_err(|error| format!("序列化行动项失败：{error}"))?;
    sqlx::query(
        "INSERT INTO meeting_minutes
            (meeting_id, markdown, quality_markdown, tasks_json, traceability_pass, validation_pass, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(meeting_id) DO UPDATE SET
            markdown = excluded.markdown,
            quality_markdown = excluded.quality_markdown,
            tasks_json = excluded.tasks_json,
            traceability_pass = excluded.traceability_pass,
            validation_pass = excluded.validation_pass,
            updated_at = excluded.updated_at",
    )
    .bind(&meeting_id)
    .bind(&output.minutes_markdown)
    .bind(&output.quality_markdown)
    .bind(&tasks_json)
    .bind(output.traceability_pass)
    .bind(output.validation_pass)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|error| format!("保存会议纪要失败：{error}"))?;

    log::info!("会议纪要已生成：meeting_id={meeting_id}");

    Ok(MinutesRecord {
        meeting_id,
        markdown: output.minutes_markdown,
        quality_markdown: output.quality_markdown,
        tasks: output.tasks,
        traceability_pass: output.traceability_pass,
        validation_pass: output.validation_pass,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// 读回已生成的纪要；未生成时返回 None。
#[tauri::command]
pub async fn api_get_minutes<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<MinutesRecord>, String> {
    let row = sqlx::query_as::<_, (String, String, String, bool, bool, String, String)>(
        "SELECT markdown, quality_markdown, tasks_json, traceability_pass, validation_pass, created_at, updated_at
         FROM meeting_minutes WHERE meeting_id = ?",
    )
    .bind(&meeting_id)
    .fetch_optional(state.db_manager.pool())
    .await
    .map_err(|error| format!("读取会议纪要失败：{error}"))?;

    let Some((markdown, quality_markdown, tasks_json, traceability_pass, validation_pass, created_at, updated_at)) = row else {
        return Ok(None);
    };
    let tasks: Vec<TaskItem> = serde_json::from_str(&tasks_json)
        .map_err(|error| format!("纪要行动项数据损坏：{error}"))?;

    Ok(Some(MinutesRecord {
        meeting_id,
        markdown,
        quality_markdown,
        tasks,
        traceability_pass,
        validation_pass,
        created_at,
        updated_at,
    }))
}

/// 取消在途的纪要生成；返回是否确有任务被取消。
#[tauri::command]
pub async fn api_cancel_minutes(meeting_id: String) -> Result<bool, String> {
    let token = MINUTES_CANCELLATIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&meeting_id)
        .cloned();
    match token {
        Some(token) => {
            token.cancel();
            log::info!("会议纪要已请求取消：meeting_id={meeting_id}");
            Ok(true)
        }
        None => Ok(false),
    }
}

/// 把已存纪要的行动项导出为 xlsx，写入 `<app data>/exports/<meeting_id>.xlsx`。
#[tauri::command]
pub async fn api_export_minutes_excel<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<String, String> {
    validate_export_id(&meeting_id)?;

    let tasks_json: Option<String> = sqlx::query_scalar(
        "SELECT tasks_json FROM meeting_minutes WHERE meeting_id = ?",
    )
    .bind(&meeting_id)
    .fetch_optional(state.db_manager.pool())
    .await
    .map_err(|error| format!("读取会议纪要失败：{error}"))?;
    let tasks_json = tasks_json.ok_or_else(|| "该会议还没有生成纪要，无法导出".to_string())?;
    let tasks: Vec<TaskItem> = serde_json::from_str(&tasks_json)
        .map_err(|error| format!("纪要行动项数据损坏：{error}"))?;

    let bytes = build_workbook_bytes(&tasks, Utc::now().date_naive())?;
    let export_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取应用数据目录失败：{error}"))?
        .join("exports");
    std::fs::create_dir_all(&export_dir)
        .map_err(|error| format!("创建导出目录失败：{error}"))?;
    let path = export_dir.join(format!("{meeting_id}.xlsx"));
    std::fs::write(&path, bytes).map_err(|error| format!("写入 Excel 文件失败：{error}"))?;

    log::info!("会议纪要 Excel 已导出：meeting_id={meeting_id}");
    Ok(path.to_string_lossy().to_string())
}

/// 导出文件名只允许安全字符，防止路径穿越。
fn validate_export_id(meeting_id: &str) -> Result<(), String> {
    if meeting_id.trim().is_empty()
        || meeting_id.contains('/')
        || meeting_id.contains('\\')
        || meeting_id.contains("..")
    {
        return Err("会议 ID 无效，无法导出".to_string());
    }
    Ok(())
}

/// 逐字稿按录音顺序拼接；空则视为不可生成。
async fn load_transcript_text(pool: &SqlitePool, meeting_id: &str) -> Result<String, String> {
    let segments: Vec<String> = sqlx::query_scalar(
        "SELECT transcript FROM transcripts WHERE meeting_id = ?
         ORDER BY COALESCE(audio_start_time, 0) ASC, rowid ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("读取逐字稿失败：{error}"))?;

    let text = segments
        .into_iter()
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        return Err("该会议还没有逐字稿，无法生成纪要".to_string());
    }
    Ok(text)
}

#[derive(Default)]
struct MeetingMetadata {
    created_at: Option<String>,
    audio_file: Option<String>,
}

/// 读 meeting 目录的 metadata.json（缺失或损坏都按空处理）。
fn read_meeting_metadata(folder: &Path) -> MeetingMetadata {
    let Ok(raw) = std::fs::read_to_string(folder.join("metadata.json")) else {
        return MeetingMetadata::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return MeetingMetadata::default();
    };
    let read = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    MeetingMetadata {
        created_at: read("created_at"),
        audio_file: read("audio_file"),
    }
}

fn register_cancellation(meeting_id: &str, token: CancellationToken) {
    MINUTES_CANCELLATIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(meeting_id.to_string(), token);
}

fn clear_cancellation(meeting_id: &str) {
    MINUTES_CANCELLATIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(meeting_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_export_ids() {
        for id in ["", "..", "../x", "a/b", "a\\b"] {
            assert!(validate_export_id(id).is_err(), "应拒绝 {id}");
        }
        assert!(validate_export_id("meeting-abc").is_ok());
    }

    #[test]
    fn cancellation_registry_round_trips() {
        let token = CancellationToken::new();
        register_cancellation("meeting-test", token.clone());
        assert!(!token.is_cancelled());
        let found = MINUTES_CANCELLATIONS
            .lock()
            .unwrap()
            .get("meeting-test")
            .cloned();
        assert!(found.is_some());
        found.unwrap().cancel();
        assert!(token.is_cancelled());
        clear_cancellation("meeting-test");
        assert!(!MINUTES_CANCELLATIONS.lock().unwrap().contains_key("meeting-test"));
    }
}
