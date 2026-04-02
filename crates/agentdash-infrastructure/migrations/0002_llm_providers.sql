-- 新建 LLM Provider 配置表，替代 settings KV 中的 llm.* 键
CREATE TABLE IF NOT EXISTS llm_providers (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    slug           TEXT NOT NULL UNIQUE,
    protocol       TEXT NOT NULL,
    api_key        TEXT NOT NULL DEFAULT '',
    base_url       TEXT NOT NULL DEFAULT '',
    wire_api       TEXT NOT NULL DEFAULT '',
    default_model  TEXT NOT NULL DEFAULT '',
    models         TEXT NOT NULL DEFAULT '[]',
    blocked_models TEXT NOT NULL DEFAULT '[]',
    env_api_key    TEXT NOT NULL DEFAULT '',
    discovery_url  TEXT NOT NULL DEFAULT '',
    sort_order     INTEGER NOT NULL DEFAULT 0,
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
