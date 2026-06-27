-- AgentRun-scoped Canvas runtime observation 与 interaction latest state。
-- RuntimeSession 只作为派生 delivery trace 诊断字段，不作为 ownership key。

CREATE TABLE IF NOT EXISTS agent_run_canvas_runtime_observations (
    id text PRIMARY KEY,
    run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    canvas_id text NOT NULL REFERENCES canvases(id) ON DELETE CASCADE,
    canvas_mount_id text NOT NULL,
    agent_run_canvas_ref text NOT NULL,
    delivery_trace_ref text,
    current_agent_frame_id text REFERENCES agent_frames(id) ON DELETE SET NULL,
    frame_id text NOT NULL,
    generation bigint NOT NULL DEFAULT 0,
    status text NOT NULL,
    payload text NOT NULL,
    captured_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT agent_run_canvas_runtime_observations_latest_unique
        UNIQUE (run_id, agent_id, canvas_mount_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_run_canvas_runtime_observations_canvas
    ON agent_run_canvas_runtime_observations (canvas_id);

CREATE TABLE IF NOT EXISTS agent_run_canvas_interaction_snapshots (
    id text PRIMARY KEY,
    run_id text NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    agent_id text NOT NULL REFERENCES lifecycle_agents(id) ON DELETE CASCADE,
    canvas_id text NOT NULL REFERENCES canvases(id) ON DELETE CASCADE,
    canvas_mount_id text NOT NULL,
    agent_run_canvas_ref text NOT NULL,
    delivery_trace_ref text,
    current_agent_frame_id text REFERENCES agent_frames(id) ON DELETE SET NULL,
    frame_id text NOT NULL,
    payload text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT agent_run_canvas_interaction_snapshots_latest_unique
        UNIQUE (run_id, agent_id, canvas_mount_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_run_canvas_interaction_snapshots_canvas
    ON agent_run_canvas_interaction_snapshots (canvas_id);
