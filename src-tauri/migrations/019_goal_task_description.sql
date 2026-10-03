-- 目标/任务描述（目标树节点展示详细内容或要求）
--
-- 1. goals 增加 description 列：描述目标的详细内容、要求或达成标准
-- 2. tasks 增加 description 列：描述任务的详细内容或要求
-- 旧数据该列为 NULL，前端按"无描述"处理，不渲染任何描述元素

ALTER TABLE goals ADD COLUMN description TEXT;
ALTER TABLE tasks ADD COLUMN description TEXT;
