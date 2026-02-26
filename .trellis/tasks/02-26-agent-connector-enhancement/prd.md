# Agent 连接器增强：支持灵活模型/模式选择和快速切换

## 背景

当前 AgentDashboard 的连接器架构虽然具备基础的分层设计，但相比 vibe-kanban 的执行器系统，在以下方面存在差距：

1. **模型选择体验**：前端使用简单的文本输入框，没有结构化选择器
2. **配置管理**：缺乏分层配置系统（Defaults → User Overrides → Session Overrides）
3. **动态发现**：没有实时发现可用模型、agents 的能力
4. **快速切换**：不支持通过选项快速切换连接类型和配置

本任务参考 vibe-kanban 的成熟设计，完善 AgentDashboard 的连接器系统。

---

## 目标

构建一个灵活、可扩展的 Agent 连接器系统，支持：
1. **多类型连接器**：本地执行器、远程 ACP 后端、未来可能的 MCP 连接器
2. **结构化模型选择**：Provider → Model → Mode 三级选择器
3. **配置变体系统**：DEFAULT、PLAN、ROUTER 等预设配置快速切换
4. **动态发现能力**：实时获取可用模型、agents、权限策略
5. **统一 Override 机制**：会话级覆盖配置参数

---

## 需求分析

### 1. 连接器架构扩展

#### 当前状态
- 仅有 `VibeKanbanExecutorsConnector` 单一实现
- Backend 配置与 Execution 分离，未打通

#### 目标设计
```rust
// 连接器 trait 层次
pub trait AgentConnector: Send + Sync {
    fn connector_id(&self) -> &'static str;
    fn connector_type(&self) -> ConnectorType;

    // 核心执行
    async fn prompt(&self, ...)
    async fn cancel(&self, ...)

    // 发现能力（新增）
    async fn discover_capabilities(&self) -> ConnectorCapabilities;
    async fn discover_options(&self) -> DiscoveryStream;

    // 配置管理（新增）
    fn get_preset_configs(&self) -> Vec<PresetConfig>;
    fn apply_overrides(&self, config: &mut ConnectorConfig, overrides: &ExecutorConfig);
}

// 连接器类型枚举
pub enum ConnectorType {
    LocalExecutor,      // 本地子进程执行器（Claude Code, Codex 等）
    RemoteAcpBackend,   // 远程 ACP 后端
    McpServer,          // 未来：MCP 服务器
}
```

### 2. 配置系统重构

#### 参考 vibe-kanban 的分层配置

```rust
// ExecutorConfig - 统一的配置接口
pub struct ExecutorConfig {
    pub executor: ExecutorType,           // CLAUDE_CODE, CODEX, etc.
    pub variant: Option<String>,          // "DEFAULT", "PLAN", "ROUTER"
    pub model_id: Option<String>,         // 模型覆盖
    pub agent_id: Option<String>,         // Agent 模式覆盖
    pub reasoning_id: Option<String>,    // 推理强度
    pub permission_policy: Option<PermissionPolicy>, // AUTO, SUPERVISED, PLAN
}

// ConnectorConfig - 连接器特定配置
pub struct ConnectorConfig {
    pub connector_id: String,
    pub connector_type: ConnectorType,
    pub endpoint: Option<String>,
    pub auth_token: Option<String>,
    pub default_executor: Option<ExecutorType>,
    pub executor_overrides: HashMap<ExecutorType, ExecutorVariantConfig>,
}
```

### 3. 模型选择器 UI

#### 三级选择器设计

参考 `vibe-kanban/packages/ui/src/components/ModelSelectorPopover.tsx`：

```typescript
interface ModelSelectorProps {
  // 输入
  providers: ModelProvider[];
  models: ModelInfo[];
  agents: AgentInfo[];
  permissions: PermissionPolicy[];

  // 当前选择
  selectedProvider?: string;
  selectedModel?: string;
  selectedReasoning?: string;
  selectedAgent?: string;
  selectedPermission?: PermissionPolicy;

  // 回调
  onChange: (config: Partial<ExecutorConfig>) => void;
}
```

#### 选择器特性
- **Accordion 分组**：按 Provider 分组折叠
- **最近使用**：LRU 缓存最近选择的模型
- **搜索过滤**：实时搜索模型名称
- **键盘导航**：完整键盘支持
- **状态回退**：选择不可用时自动回退到默认值

### 4. 动态发现系统

#### 后端发现接口

```rust
// 发现选项流
pub struct DiscoveredOptions {
    pub model_selector: ModelSelectorConfig,
    pub available_executors: Vec<ExecutorInfo>,
    pub slash_commands: Vec<SlashCommand>,
    pub loading: LoadingState,
    pub error: Option<String>,
}

// 流式更新（SSE/WebSocket）
pub type DiscoveryStream = BoxStream<'static, JsonPatch>;
```

#### 前端 Hook

```typescript
// useConnectorDiscovery.ts
interface UseConnectorDiscoveryResult {
  options: DiscoveredOptions | null;
  loading: boolean;
  error: Error | null;
  refresh: () => void;
}

function useConnectorDiscovery(
  connectorId: string,
  workspaceId?: string
): UseConnectorDiscoveryResult;
```

### 5. 快速切换 UI

#### Agent 选择器

参考 `vibe-kanban/packages/web-core/src/shared/components/tasks/AgentSelector.tsx`：

```typescript
interface AgentSelectorProps {
  availableAgents: ExecutorType[];
  selectedAgent: ExecutorType | null;
  onChange: (agent: ExecutorType) => void;
}
```

#### 配置变体选择器

```typescript
interface ConfigVariantSelectorProps {
  agent: ExecutorType;
  availableVariants: string[];  // ["DEFAULT", "PLAN", "ROUTER"]
  selectedVariant: string;
  onChange: (variant: string) => void;
}
```

---

## 任务拆分

### Phase 1: 后端基础架构

- [ ] **1.1** 扩展 `AgentConnector` trait，添加发现能力和配置管理
- [ ] **1.2** 实现 `ConnectorType` 枚举和类型系统
- [ ] **1.3** 重构 `ExecutorConfig`，支持变体和覆盖
- [ ] **1.4** 添加 `ModelSelectorConfig` 数据结构
- [ ] **1.5** 实现 `discover_options` 流式接口（SSE）

### Phase 2: 后端连接器实现

- [ ] **2.1** 重构 `VibeKanbanExecutorsConnector`，支持多种执行器类型
- [ ] **2.2** 实现 `RemoteAcpConnector`（远程 ACP 后端连接）
- [ ] **2.3** 实现 `LocalExecutorRegistry`（本地执行器注册表）
- [ ] **2.4** 添加配置持久化（SQLite）

### Phase 3: 前端基础组件

- [ ] **3.1** 实现 `ModelSelector` 组件（三级选择器）
- [ ] **3.2** 实现 `AgentSelector` 组件（执行器类型选择）
- [ ] **3.3** 实现 `ConfigVariantSelector` 组件（配置变体切换）
- [ ] **3.4** 实现 `PermissionSelector` 组件（权限策略选择）

### Phase 4: 前端集成

- [ ] **4.1** 实现 `useConnectorDiscovery` hook
- [ ] **4.2** 重构 Session 创建页面，集成新选择器
- [ ] **4.3** 实现连接器配置持久化（localStorage）
- [ ] **4.4** 添加最近使用模型追踪

### Phase 5: 端到端联调

- [ ] **5.1** 联调本地执行器 + 模型选择
- [ ] **5.2** 联调远程 ACP 连接器
- [ ] **5.3** 验证配置覆盖机制
- [ ] **5.4** 性能优化（缓存、防抖）

---

## 技术方案

### 后端架构

```
crates/agentdash-api/src/executor/
├── connector.rs              # AgentConnector trait（扩展）
├── connector_type.rs         # ConnectorType 枚举
├── config.rs                 # ConnectorConfig, ExecutorConfig
├── discovery.rs              # 发现系统接口
├── hub.rs                    # ExecutorHub（重构）
├── connectors/
│   ├── mod.rs                # 连接器注册表
│   ├── local/
│   │   ├── mod.rs            # LocalExecutorRegistry
│   │   ├── claude_code.rs    # Claude Code 特定配置
│   │   ├── codex.rs          # Codex 特定配置
│   │   └── ...
│   └── remote/
│       └── acp_backend.rs    # Remote ACP 连接器
└── model_selector.rs         # 模型选择器配置（从 vibe-kanban 复用）
```

### 前端架构

```
frontend/src/features/connector/
├── model-selector/
│   ├── ModelSelector.tsx         # 主选择器组件
│   ├── ModelSelectorPopover.tsx  # Popover 弹窗
│   ├── ProviderAccordion.tsx     # Provider 折叠面板
│   └── useModelSelector.ts       # 选择器状态管理
├── agent-selector/
│   ├── AgentSelector.tsx         # 执行器类型选择
│   └── useAgentSelector.ts
├── config-selector/
│   ├── ConfigVariantSelector.tsx # 配置变体选择
│   └── useConfigVariants.ts
└── discovery/
    ├── useConnectorDiscovery.ts  # 发现 hook
    └── discoveryStream.ts        # 流处理
```

---

## 验收标准

### 功能验收

- [ ] 用户可以在创建 Session 时选择不同的 Agent 类型（Claude Code, Codex 等）
- [ ] 用户可以通过三级选择器选择 Provider → Model → Reasoning
- [ ] 用户可以快速切换配置变体（DEFAULT / PLAN / ROUTER）
- [ ] 用户可以选择权限策略（AUTO / SUPERVISED / PLAN）
- [ ] 系统实时显示可用的模型和配置选项
- [ ] 最近使用的模型会出现在选择器顶部
- [ ] 配置可以保存并在新 Session 中复用

### 性能验收

- [ ] 模型选择器首次加载 < 1s
- [ ] 切换 Agent 类型时配置选项实时更新
- [ ] 搜索过滤响应 < 100ms

### 兼容性验收

- [ ] 向后兼容现有 Session 创建 API
- [ ] 不支持新特性的旧连接器优雅降级

---

## 参考文档

### vibe-kanban 关键文件

| 功能 | 文件路径 |
|------|----------|
| Executor Trait | `crates/executors/src/executors/mod.rs` |
| Profile 系统 | `crates/executors/src/profile.rs` |
| Model Selector | `crates/executors/src/model_selector.rs` |
| 发现系统 | `crates/executors/src/executor_discovery.rs` |
| Model Selector UI | `packages/ui/src/components/ModelSelectorPopover.tsx` |
| Agent Selector | `packages/web-core/src/shared/components/tasks/AgentSelector.tsx` |

### AgentDashboard 相关文件

| 功能 | 文件路径 |
|------|----------|
| Connector Trait | `crates/agentdash-api/src/executor/connector.rs` |
| Executor Hub | `crates/agentdash-api/src/executor/hub.rs` |
| VibeKanban Connector | `crates/agentdash-api/src/executor/connectors/vibe_kanban.rs` |
| Session 页面 | `frontend/src/pages/SessionPage.tsx` |

---

## 风险与注意事项

1. **与现有 SSE 流改造的关系**：当前正在进行的 SSE 流改造可能影响发现接口的设计，需要协调
2. **配置迁移**：现有 Session 配置需要迁移到新格式
3. **执行器可用性检测**：需要检测本地是否安装了 Claude Code、Codex 等工具
4. **远程连接器安全**：远程 ACP 连接器需要处理好认证和授权
