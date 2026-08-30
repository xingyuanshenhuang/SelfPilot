-- 增加鼓励语风格标签字段，用于「温暖鼓励」风格偏好过滤
ALTER TABLE encouragements ADD COLUMN style TEXT NOT NULL DEFAULT 'warm';