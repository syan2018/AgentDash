-- 收口到 Activity 唯一 lifecycle 模型后，删除旧 Step 轨残留列。
-- 数据已由 0050 迁移至 Activity 形态，应用层（P3b）已不再读写这些列。
ALTER TABLE lifecycle_definitions DROP COLUMN IF EXISTS entry_step_key;
ALTER TABLE lifecycle_definitions DROP COLUMN IF EXISTS steps;
ALTER TABLE lifecycle_definitions DROP COLUMN IF EXISTS edges;
ALTER TABLE lifecycle_runs DROP COLUMN IF EXISTS step_states;
