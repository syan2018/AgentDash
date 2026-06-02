-- Phase 5: lifecycle_id → root_graph_id
-- LifecycleRun 的 lifecycle_id 实际指向 root WorkflowGraph，重命名以消除歧义。
ALTER TABLE lifecycle_runs RENAME COLUMN lifecycle_id TO root_graph_id;
