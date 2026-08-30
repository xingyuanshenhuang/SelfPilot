-- 为非 warm 风格的预设文案补充"专业理性"与"极简克制"标签
-- 使"文案风格"设置真正可观测（配合 encouragement.rs 的 style 过滤）
-- 采用新增迁移而非改写 016，避免触发 SQLx checksum 校验失败

UPDATE encouragements SET style = 'professional'
WHERE category = 'preset' AND id IN (
  'preset-p1-n04', 'preset-p1-n07',
  'preset-p1-a06', 'preset-p1-a11', 'preset-p1-a14',
  'preset-p1-h11', 'preset-p1-h13', 'preset-p1-h15',
  'preset-p1-c02', 'preset-p1-c05'
);

UPDATE encouragements SET style = 'minimal'
WHERE category = 'preset' AND id IN (
  'preset-p1-n02', 'preset-p1-n03', 'preset-p1-n05', 'preset-p1-n06',
  'preset-p1-a01', 'preset-p1-a10',
  'preset-p1-h07', 'preset-p1-h09', 'preset-p1-h10',
  'preset-p1-s08'
);