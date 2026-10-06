-- 重复任务系列描述
--
-- repeat_series 增加 description 列：保存"添加任务-重复任务"时的任务描述，
-- 供"编辑原任务"预填表单并级联更新所有实例。
-- 旧数据该列为 NULL，前端按"无描述"处理。
-- tasks.description 列已由 019 迁移提供，实例描述随创建/级联更新写入。

ALTER TABLE repeat_series ADD COLUMN description TEXT;
