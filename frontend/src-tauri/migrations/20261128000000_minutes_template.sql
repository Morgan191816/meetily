-- 记录每条纪要生成时使用的模板；旧纪要保持 NULL，读取时返回 None。
ALTER TABLE meeting_minutes ADD COLUMN template_id TEXT;

-- 默认纪要模板 id（settings 为单行表，沿用既有列式存储）。
ALTER TABLE settings ADD COLUMN minutesTemplate TEXT;
