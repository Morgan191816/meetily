-- 说话人显示名：label 为 transcripts.speaker 里的稳定标签
CREATE TABLE IF NOT EXISTS meeting_speakers (
    meeting_id   TEXT NOT NULL,
    label        TEXT NOT NULL,
    display_name TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    PRIMARY KEY (meeting_id, label)
);
