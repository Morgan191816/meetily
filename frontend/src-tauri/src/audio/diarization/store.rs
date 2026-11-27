//! 说话人显示名映射：`transcripts.speaker` 里的标签 → 用户可读姓名。

use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerRow {
    pub label: String,
    pub display_name: Option<String>,
}

/// 该会议出现过的所有说话人标签，左连接已保存的显示名。
pub async fn list_speakers(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<SpeakerRow>, String> {
    sqlx::query_as::<_, SpeakerRow>(
        "SELECT d.speaker AS label, s.display_name
         FROM (SELECT DISTINCT speaker FROM transcripts
               WHERE meeting_id = ?1 AND speaker IS NOT NULL) d
         LEFT JOIN meeting_speakers s ON s.meeting_id = ?1 AND s.label = d.speaker
         ORDER BY d.speaker ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("读取说话人失败：{error}"))
}

/// 保存显示名；空名表示清除映射。
pub async fn set_display_name(
    pool: &SqlitePool,
    meeting_id: &str,
    label: &str,
    name: &str,
) -> Result<(), String> {
    // 与前端 maxLength 一致：按字符截断，避免把多字节姓名截坏。
    let name: String = name.trim().chars().take(30).collect();
    if name.is_empty() {
        sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ? AND label = ?")
            .bind(meeting_id)
            .bind(label)
            .execute(pool)
            .await
            .map_err(|error| format!("清除说话人名称失败：{error}"))?;
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO meeting_speakers (meeting_id, label, display_name, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(meeting_id, label) DO UPDATE SET display_name = excluded.display_name,
                                                     updated_at = excluded.updated_at",
    )
    .bind(meeting_id)
    .bind(label)
    .bind(name)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .map_err(|error| format!("保存说话人名称失败：{error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE transcripts (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                transcript TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                speaker TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE meeting_speakers (
                meeting_id TEXT NOT NULL,
                label TEXT NOT NULL,
                display_name TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (meeting_id, label)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        for (id, speaker) in [
            ("t1", "SPEAKER_00"),
            ("t2", "SPEAKER_01"),
            ("t3", "SPEAKER_00"),
        ] {
            sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, speaker)
                 VALUES (?, 'm1', 'x', '00:00', ?)",
            )
            .bind(id)
            .bind(speaker)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn set_then_clear_display_name() {
        let pool = test_pool().await;

        let rows = list_speakers(&pool, "m1").await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "SPEAKER_00");
        assert_eq!(rows[0].display_name, None);

        set_display_name(&pool, "m1", "SPEAKER_00", " 张三 ")
            .await
            .unwrap();
        let rows = list_speakers(&pool, "m1").await.unwrap();
        assert_eq!(rows[0].display_name.as_deref(), Some("张三"));

        // 覆盖已有名称
        set_display_name(&pool, "m1", "SPEAKER_00", "李四")
            .await
            .unwrap();
        let rows = list_speakers(&pool, "m1").await.unwrap();
        assert_eq!(rows[0].display_name.as_deref(), Some("李四"));

        // 空名清除映射
        set_display_name(&pool, "m1", "SPEAKER_00", "  ")
            .await
            .unwrap();
        let rows = list_speakers(&pool, "m1").await.unwrap();
        assert_eq!(rows[0].display_name, None);
    }

    #[tokio::test]
    async fn truncates_display_name_to_30_chars() {
        let pool = test_pool().await;
        let long_name = "名".repeat(40);
        set_display_name(&pool, "m1", "SPEAKER_00", &long_name)
            .await
            .unwrap();
        let rows = list_speakers(&pool, "m1").await.unwrap();
        assert_eq!(rows[0].display_name, Some("名".repeat(30)));
    }
}
