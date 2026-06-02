-- workflow_definitions / lifecycle_definitions 的 status 列自 0013 起已不再被代码读写，
-- 但 0001_init 里仍是 NOT NULL 约束，导致新 INSERT（未显式提供 status）触发
--   "null value in column \"status\" violates not-null constraint"
-- 这里只放开 NOT NULL 约束，保留列（避免破坏历史 SELECT 和潜在外部只读工具）。

ALTER TABLE workflow_definitions ALTER COLUMN status DROP NOT NULL;
ALTER TABLE lifecycle_definitions ALTER COLUMN status DROP NOT NULL;
