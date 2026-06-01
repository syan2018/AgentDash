-- B1 Target Anchor Schema: 控制面目标锚点 schema
-- 建立 WorkflowGraphInstance / LifecycleAgent / AgentFrame / AgentAssignment /
-- LifecycleSubjectAssociation / LifecycleGate / AgentLineage 7 张目标表。

-- ═══════════════════════════════════════════════════════════════════════════════
-- 1. lifecycle_workflow_instances
--    graph 在某个 LifecycleRun 内的一次生效实例
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS lifecycle_workflow_instances (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    graph_id TEXT NOT NULL,
    role TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    activity_state_json TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_lwi_run_id
    ON lifecycle_workflow_instances(run_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_lwi_run_root
    ON lifecycle_workflow_instances(run_id, role)
    WHERE role = 'root';

-- ═══════════════════════════════════════════════════════════════════════════════
-- 2. lifecycle_agents
--    Run-scoped Agent runtime identity
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS lifecycle_agents (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL,
    agent_kind TEXT NOT NULL,
    agent_role TEXT NOT NULL DEFAULT 'primary',
    project_agent_id TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    current_frame_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_agents_run_id
    ON lifecycle_agents(run_id);

CREATE INDEX IF NOT EXISTS idx_lifecycle_agents_project_id
    ON lifecycle_agents(project_id);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 3. agent_frames
--    AgentFrame revision row — effective runtime surface
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS agent_frames (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL DEFAULT 1,
    procedure_id TEXT,
    graph_instance_id TEXT,
    activity_key TEXT,
    effective_capability_json TEXT,
    context_slice_json TEXT,
    vfs_surface_json TEXT,
    mcp_surface_json TEXT,
    runtime_session_refs_json TEXT,
    created_by_kind TEXT NOT NULL DEFAULT 'backfill',
    created_by_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_frames_agent_revision
    ON agent_frames(agent_id, revision);

CREATE INDEX IF NOT EXISTS idx_agent_frames_agent_id
    ON agent_frames(agent_id);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 4. agent_assignments
--    Agent/Frame → graph activity attempt 的执行桥
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS agent_assignments (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    graph_instance_id TEXT NOT NULL,
    activity_key TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    agent_id TEXT NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    frame_id TEXT NOT NULL REFERENCES agent_frames(id) ON DELETE CASCADE,
    lease_status TEXT NOT NULL DEFAULT 'active',
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    released_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_agent_assignments_run_id
    ON agent_assignments(run_id);

CREATE INDEX IF NOT EXISTS idx_agent_assignments_graph_activity
    ON agent_assignments(graph_instance_id, activity_key, attempt);

CREATE INDEX IF NOT EXISTS idx_agent_assignments_agent_id
    ON agent_assignments(agent_id);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 5. lifecycle_subject_associations
--    SubjectRef → whole run 或 LifecycleAgent 的关系
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS lifecycle_subject_associations (
    id TEXT PRIMARY KEY,
    anchor_run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    anchor_agent_id TEXT,
    subject_kind TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    role TEXT NOT NULL,
    metadata_json TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_lsa_anchor_run
    ON lifecycle_subject_associations(anchor_run_id);

CREATE INDEX IF NOT EXISTS idx_lsa_anchor_agent
    ON lifecycle_subject_associations(anchor_agent_id)
    WHERE anchor_agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_lsa_subject
    ON lifecycle_subject_associations(subject_kind, subject_id);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 6. lifecycle_gates
--    Durable wait/review/resume 点
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS lifecycle_gates (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id TEXT,
    frame_id TEXT,
    gate_kind TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    payload_json TEXT,
    resolved_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_gates_run_id
    ON lifecycle_gates(run_id);

CREATE INDEX IF NOT EXISTS idx_lifecycle_gates_agent_status
    ON lifecycle_gates(agent_id, status)
    WHERE agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_lifecycle_gates_correlation
    ON lifecycle_gates(correlation_id);

-- ═══════════════════════════════════════════════════════════════════════════════
-- 7. agent_lineages
--    Agent spawn/delegation/companion relation (控制树)
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS agent_lineages (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    parent_agent_id TEXT,
    child_agent_id TEXT NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    relation_kind TEXT NOT NULL,
    source_frame_id TEXT,
    metadata_json TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_agent_lineages_run_id
    ON agent_lineages(run_id);

CREATE INDEX IF NOT EXISTS idx_agent_lineages_parent
    ON agent_lineages(parent_agent_id)
    WHERE parent_agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_agent_lineages_child
    ON agent_lineages(child_agent_id);
