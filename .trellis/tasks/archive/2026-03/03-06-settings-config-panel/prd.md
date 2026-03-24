# Pi Agent 设置与配置面板

## 1. 背景与目标

当前 Pi Agent（agentdash-agent）的初始化依赖环境变量（`ANTHROPIC_API_KEY`、`OPENAI_API_KEY`、`PI_AGENT_SYSTEM_PROMPT`），无法通过 UI 配置。同时，项目缺乏一个统一的全局设置页面——配置分散在项目选择器、executor-selector 的 localStorage、环境变量等多处。

本任务的目标是：
1. 建立统一的全局设置存储与 API
2. 实现前端设置页面（`/settings`），集中管理 LLM Provider 密钥、模型偏好、Pi Agent 参数
3. 让 Pi Agent 初始化从持久化设置中读取，而非仅依赖环境变量

## 2. Scope / Trigger

本任务涉及跨层变更：
- 新增后端设置 API（CRUD）
- 新增 Settings 数据模型（或利用现有 Backend/ProjectConfig 扩展）
- 新增前端 `/settings` 路由与页面
- 修改 `AppState` 中 PiAgentConnector 的初始化逻辑
- 修改侧边栏导航，增加设置入口

## 3. Goals / Non-Goals

### Goals

- **G1**: 全局设置存储——支持 LLM Provider 配置（API Key、Base URL、默认模型）
- **G2**: 前端设置页面——清晰分区的表单 UI，支持多 Provider、Pi Agent 参数
- **G3**: Pi Agent 动态初始化——从设置读取，支持热切换（不重启服务）
- **G4**: 执行器默认配置——全局默认 executor、variant、model 偏好持久化到服务端
- **G5**: 安全性——API Key 存储后在 GET 响应中脱敏显示（`sk-...****`）

### Non-Goals

- 不做多用户/多租户的设置隔离（当前单用户场景）
- 不做 OAuth/SSO 等认证集成
- 不做设置的导入/导出（后续可扩展）
- 不做实时校验 API Key 有效性（仅格式检查）

## 4. ADR-lite（核心决策）

### 决策 A：设置存储采用 Key-Value 模型

使用 `settings` 表，schema 为 `(key TEXT PRIMARY KEY, value TEXT, updated_at)`。value 存 JSON 字符串。
这比强类型列更灵活，适合频繁变化的配置项。

设置 key 命名规范：`{category}.{subcategory}.{field}`
- `llm.anthropic.api_key`
- `llm.anthropic.default_model`
- `llm.openai.api_key`
- `llm.openai.default_model`
- `agent.pi.system_prompt`
- `agent.pi.temperature`
- `agent.pi.max_turns`
- `executor.default.executor`
- `executor.default.variant`
- `executor.default.model_id`

### 决策 B：API Key 脱敏策略

- 写入时存储完整值
- 读取时返回脱敏值（前4位 + `****` + 后4位），除非请求带 `reveal=true`（未来可加权限控制）
- 前端如果未修改脱敏值则不发送该字段（patch 语义）

### 决策 C：Pi Agent 热切换

通过 `Arc<RwLock<Option<PiAgentConnector>>>` 持有 PiAgent 引用。设置更新后，重建 connector 并原子替换。CompositeConnector 的路由表同步刷新。

### 决策 D：设置页面布局

独立 `/settings` 路由，分 Tab 组织：
- **LLM Providers**：Anthropic / OpenAI / 其他，每个 Provider 一个 card
- **Pi Agent**：System Prompt、Temperature、Max Turns
- **默认执行器**：全局默认的 executor / variant / model 偏好
- **外观**：主题（从 ThemeToggle 迁移）

## 5. Signatures

### 5.1 数据模型

```sql
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### 5.2 后端 API

```rust
// GET /api/settings
// 返回所有设置（API Key 脱敏）
// Query: ?category=llm  （可选过滤）
struct SettingsListResponse {
    settings: Vec<SettingEntry>,
}

struct SettingEntry {
    key: String,
    value: serde_json::Value,
    updated_at: String,
    masked: bool,  // 标识该值是否已脱敏
}

// PUT /api/settings
// 批量更新设置（patch 语义，只更新提供的 key）
struct UpdateSettingsRequest {
    settings: Vec<SettingUpdate>,
}

struct SettingUpdate {
    key: String,
    value: serde_json::Value,
}

struct UpdateSettingsResponse {
    updated: Vec<String>,  // 更新的 key 列表
    errors: Vec<SettingError>,
}

// DELETE /api/settings/{key}
// 删除单个设置项（恢复默认）
```

### 5.3 Domain 层

```rust
// crates/agentdash-domain/src/settings.rs
pub struct Setting {
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: DateTime<Utc>,
}

pub trait SettingsRepository: Send + Sync {
    async fn list(&self, category_prefix: Option<&str>) -> Result<Vec<Setting>>;
    async fn get(&self, key: &str) -> Result<Option<Setting>>;
    async fn set(&self, key: &str, value: serde_json::Value) -> Result<()>;
    async fn set_batch(&self, entries: &[(String, serde_json::Value)]) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<bool>;
}
```

### 5.4 Infrastructure 层

```rust
// crates/agentdash-infrastructure/src/persistence/sqlite/settings_repository.rs
pub struct SqliteSettingsRepository { pool: SqlitePool }
impl SettingsRepository for SqliteSettingsRepository { ... }
```

### 5.5 前端类型

```ts
interface SettingEntry {
  key: string;
  value: unknown;
  updated_at: string;
  masked: boolean;
}

interface UpdateSettingsPayload {
  settings: Array<{ key: string; value: unknown }>;
}

// Store
interface SettingsState {
  settings: Map<string, SettingEntry>;
  loading: boolean;
  error: string | null;
  
  fetchSettings: (category?: string) => Promise<void>;
  updateSettings: (updates: Array<{ key: string; value: unknown }>) => Promise<void>;
  deleteSetting: (key: string) => Promise<void>;
  getSetting: (key: string) => SettingEntry | undefined;
}
```

### 5.6 前端路由 & 组件

```
frontend/src/
├── pages/
│   └── SettingsPage.tsx           # /settings 路由页面
├── features/settings/
│   ├── ui/
│   │   ├── SettingsLayout.tsx     # Tab 布局
│   │   ├── LlmProvidersTab.tsx    # LLM 配置
│   │   ├── PiAgentTab.tsx         # Pi Agent 参数
│   │   ├── DefaultExecutorTab.tsx # 默认执行器
│   │   └── AppearanceTab.tsx      # 外观（主题）
│   └── model/
│       └── useSettings.ts         # 设置相关 hooks
├── stores/
│   └── settingsStore.ts           # Zustand store
└── api/
    └── settings.ts                # API 客户端
```

## 6. Contracts

### 6.1 API Key 脱敏规则

| 原始值 | 脱敏结果 | 规则 |
|--------|----------|------|
| `sk-ant-api03-abc...xyz` | `sk-a...xyz` | 前4字符 + `...` + 后4字符 |
| 短于8字符 | `****` | 全部遮蔽 |
| 空/null | `null` | 不脱敏 |

### 6.2 设置变更触发 Pi Agent 重建

```
PUT /api/settings (含 llm.*.api_key 或 agent.pi.*)
  → 更新数据库
  → 触发 PiAgentConnector 重建
  → 刷新 CompositeConnector 路由表
  → 返回成功
```

### 6.3 设置优先级（Pi Agent 初始化）

1. 数据库中的 settings（最高优先）
2. 环境变量（`ANTHROPIC_API_KEY` 等，fallback）
3. 代码默认值（最低优先）

## 7. Validation & Error Matrix

| 场景 | 接口 | 错误码 | 错误信息 |
|------|------|--------|----------|
| key 格式非法（不含`.`） | PUT /settings | 400 | `无效的设置 key 格式` |
| 未知 category | GET /settings?category=xxx | 200 | 返回空列表 |
| 删除不存在的 key | DELETE /settings/{key} | 404 | `设置项 {key} 不存在` |
| API Key 格式异常 | PUT /settings | 400 | `API Key 格式无效` |
| 数据库写入失败 | PUT /settings | 500 | `设置保存失败` |

## 8. Good / Base / Bad Cases

### Good
- 用户打开 `/settings`，看到当前所有配置（API Key 已脱敏）
- 输入 Anthropic API Key，保存后 Pi Agent 自动可用
- 在 Agent Discovery 中看到 `pi-agent` 执行器
- 修改 system prompt 后，新会话立即使用新配置

### Base
- 未配置任何 API Key：Pi Agent 不出现在执行器列表中
- 只配置了 OpenAI：Pi Agent 使用 gpt-5.4
- 修改 API Key 为无效值：保存成功，但 Pi Agent 调用时返回 LLM 错误

### Bad
- 设置页面加载失败：显示错误提示，不影响其他功能
- 数据库不可写：返回 500，前端显示保存失败

## 9. 验收标准

- [ ] `settings` 表创建，CRUD API 正常工作
- [ ] API Key 在 GET 响应中正确脱敏
- [ ] 前端 `/settings` 页面包含 LLM Providers、Pi Agent、默认执行器、外观 四个 Tab
- [ ] 保存 Anthropic API Key 后，Pi Agent 自动初始化并出现在 Discovery 列表
- [ ] 修改 Pi Agent 参数（system_prompt 等）后新会话使用新配置
- [ ] 删除 API Key 后 Pi Agent 从 Discovery 列表消失
- [ ] 侧边栏新增设置入口（齿轮图标）
- [ ] 环境变量作为 fallback 仍然生效

## 10. 实施拆分（建议）

### Phase 1: 后端基础（约 2h）
1. Domain 层：`Setting` 实体 + `SettingsRepository` trait
2. Infrastructure 层：`SqliteSettingsRepository` 实现
3. API 层：`GET/PUT/DELETE /api/settings` 路由
4. 脱敏逻辑

### Phase 2: Pi Agent 热切换（约 1.5h）
5. 修改 `AppState`：settings → PiAgentConnector 初始化
6. 设置变更回调：重建 connector
7. CompositeConnector 路由刷新

### Phase 3: 前端设置页面（约 3h）
8. `settingsStore` + API client
9. SettingsPage + Tab 布局
10. LLM Providers Tab（API Key 输入、模型选择）
11. Pi Agent Tab（system prompt、temperature 等）
12. 默认执行器 Tab
13. 侧边栏设置入口

### Phase 4: 集成测试（约 1h）
14. 端到端测试：保存 Key → Pi Agent 可用 → 会话测试
