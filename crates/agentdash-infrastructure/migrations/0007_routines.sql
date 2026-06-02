-- Routine 触发框架 — routines + routine_executions 表

CREATE TABLE IF NOT EXISTS routines (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    prompt_template TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    trigger_config TEXT NOT NULL,       -- JSON: RoutineTriggerConfig tagged enum
    session_strategy TEXT NOT NULL,     -- JSON: SessionStrategy
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_fired_at TEXT,

    UNIQUE(project_id, name)
);

CREATE INDEX IF NOT EXISTS idx_routines_project ON routines(project_id);
CREATE INDEX IF NOT EXISTS idx_routines_enabled ON routines(enabled);

CREATE TABLE IF NOT EXISTS routine_executions (
    id TEXT PRIMARY KEY,
    routine_id TEXT NOT NULL,
    trigger_source TEXT NOT NULL,
    trigger_payload TEXT,               -- JSON
    resolved_prompt TEXT,
    session_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    started_at TEXT NOT NULL,
    completed_at TEXT,
    error TEXT,
    entity_key TEXT                     -- PerEntity session affinity
);

CREATE INDEX IF NOT EXISTS idx_routine_exec_routine ON routine_executions(routine_id);
CREATE INDEX IF NOT EXISTS idx_routine_exec_status ON routine_executions(routine_id, status);
CREATE INDEX IF NOT EXISTS idx_routine_exec_entity ON routine_executions(routine_id, entity_key);
