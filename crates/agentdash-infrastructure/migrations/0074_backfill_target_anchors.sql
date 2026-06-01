-- B1 Backfill: 从旧 LifecycleRun/LifecycleRunLink 结构迁移到目标锚点表
-- 执行顺序：graph instance → agent → frame → subject association

-- ═══════════════════════════════════════════════════════════════════════════════
-- 1. LifecycleRun.lifecycle_id → WorkflowGraphInstance(role=root)
-- ═══════════════════════════════════════════════════════════════════════════════
INSERT INTO lifecycle_workflow_instances (id, run_id, graph_id, role, status, created_at, updated_at)
SELECT
    gen_random_uuid()::text,
    lr.id,
    lr.lifecycle_id,
    'root',
    CASE
        WHEN lr.status IN ('completed', 'failed', 'cancelled') THEN lr.status
        ELSE 'active'
    END,
    lr.created_at,
    lr.updated_at
FROM lifecycle_runs lr
WHERE NOT EXISTS (
    SELECT 1 FROM lifecycle_workflow_instances lwi
    WHERE lwi.run_id = lr.id AND lwi.role = 'root'
);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 2. LifecycleRun.session_id → root LifecycleAgent
-- ═══════════════════════════════════════════════════════════════════════════════
INSERT INTO lifecycle_agents (id, run_id, project_id, agent_kind, agent_role, status, created_at, updated_at)
SELECT
    gen_random_uuid()::text,
    lr.id,
    lr.project_id,
    'session',
    'primary',
    CASE
        WHEN lr.status IN ('completed', 'failed', 'cancelled') THEN lr.status
        ELSE 'active'
    END,
    lr.created_at,
    lr.updated_at
FROM lifecycle_runs lr
WHERE lr.session_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM lifecycle_agents la
    WHERE la.run_id = lr.id AND la.agent_role = 'primary'
);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 3. root LifecycleAgent → initial AgentFrame (runtime_session_refs)
-- ═══════════════════════════════════════════════════════════════════════════════
INSERT INTO agent_frames (id, agent_id, revision, runtime_session_refs_json, created_by_kind, created_at)
SELECT
    gen_random_uuid()::text,
    la.id,
    1,
    json_build_array(lr.session_id)::text,
    'backfill',
    la.created_at
FROM lifecycle_agents la
JOIN lifecycle_runs lr ON lr.id = la.run_id
WHERE la.agent_role = 'primary'
  AND lr.session_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM agent_frames af
    WHERE af.agent_id = la.id
);

-- 回填 current_frame_id 到 lifecycle_agents
UPDATE lifecycle_agents la
SET current_frame_id = (
    SELECT af.id FROM agent_frames af
    WHERE af.agent_id = la.id
    ORDER BY af.revision DESC
    LIMIT 1
)
WHERE la.current_frame_id IS NULL
  AND EXISTS (SELECT 1 FROM agent_frames af WHERE af.agent_id = la.id);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 4. LifecycleRunLink → whole-run LifecycleSubjectAssociation
-- ═══════════════════════════════════════════════════════════════════════════════
INSERT INTO lifecycle_subject_associations (id, anchor_run_id, subject_kind, subject_id, role, metadata_json, created_at)
SELECT
    gen_random_uuid()::text,
    lrl.run_id,
    lrl.subject_kind,
    lrl.subject_id,
    lrl.role,
    lrl.metadata::text,
    lrl.created_at
FROM lifecycle_run_links lrl
WHERE NOT EXISTS (
    SELECT 1 FROM lifecycle_subject_associations lsa
    WHERE lsa.anchor_run_id = lrl.run_id
      AND lsa.subject_kind = lrl.subject_kind
      AND lsa.subject_id = lrl.subject_id
      AND lsa.role = lrl.role
);
