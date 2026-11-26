//! 说话人相关的 Tauri 命令：列表、重命名、OA 名单。

use super::store::{list_speakers, set_display_name, SpeakerRow};
use crate::state::AppState;
use tauri::{command, Runtime};

#[command]
pub async fn api_get_meeting_speakers<R: Runtime>(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<SpeakerRow>, String> {
    list_speakers(state.db_manager.pool(), &meeting_id).await
}

#[command]
pub async fn api_rename_speaker<R: Runtime>(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    label: String,
    name: String,
) -> Result<(), String> {
    set_display_name(state.db_manager.pool(), &meeting_id, &label, &name).await
}

#[command]
pub async fn api_get_roster_names() -> Result<Vec<String>, String> {
    Ok(crate::minutes::roster::member_names().to_vec())
}
