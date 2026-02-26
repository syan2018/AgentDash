# 实现执行器/模型选择器（前后端联动）

## 背景与动机

当前 AgentDashboard 的 Session 页面（`SessionPage.tsx`）使用简单的文本输入框让用户输入 `executor` 和 `modelId`，存在以下问题：

1. **用户体验差**：用户需要记忆执行器名称（如 `CLAUDE_CODE`、`AMP`、`GEMINI` 等）
2. **容易出错**：手动输入容易产生拼写错误
3. **缺乏发现性**：无法看到所有可用的执行器和模型选项
4. **配置不完整**：缺少 `variant`、`agentId`、`reasoningId`、`permissionPolicy` 等高级配置

参考 vibe-kanban 的实现，用户期望：
- 直观的下拉选择器展示所有可用执行器
- 按提供商分组的模型列表
- 配置持久化（记住上次选择）

## 目标（Goal）

实现完整的执行器/模型选择器功能，包括：

1. **后端**：提供 WebSocket 端点 `/api/agents/discovered-options/ws`，实时推送可用执行器和模型列表
2. **前端**：在 SessionPage 中集成模型选择器 UI，支持选择执行器、模型和高级配置
3. **体验**：配置持久化、加载状态、错误处理

---

## 范围（Scope）

### In Scope

**后端**：
- [ ] 新增 WebSocket 端点 `/api/agents/discovered-options/ws`
- [ ] 从 vibe-kanban 的 executor 系统中读取可用执行器列表
- [ ] 支持 SSE/WebSocket 流式推送发现选项
- [ ] 支持动态刷新（当配置变化时推送更新）

**前端**：
- [ ] 创建 `ModelSelector` 组件（基于 vibe-kanban 的 `ModelSelectorPopover`）
- [ ] 创建 `useExecutorDiscovery` Hook 连接后端 WebSocket
- [ ] 创建 `useExecutorConfig` Hook 管理配置状态 + 持久化
- [ ] 在 `SessionPage` 中替换现有的文本输入框
- [ ] 支持展示：执行器列表、模型列表（按提供商分组）、变体选择、权限策略

### Out of Scope

- 执行器配置的细粒度验证（如某些模型只在特定执行器可用）
- 自定义执行器配置文件的 UI 编辑
- 多后端（backendId）维度的执行器隔离（后续任务）

---

## 关键契约与约束（Contract & Constraints）

### 1) WebSocket API：获取可用执行器/模型

**端点**: `GET /api/agents/discovered-options/ws`

**消息格式**（服务端 → 客户端）：
```json
{
  "type": "discovery",
  "data": {
    "providers": [
      { "id": "anthropic", "name": "Anthropic", "icon": "..." },
      { "id": "google", "name": "Google", "icon": "..." }
    ],
    "models": [
      {
        "id": "claude-3-5-sonnet-20241022",
        "name": "Claude 3.5 Sonnet",
        "provider": "anthropic",
        "capabilities": ["vision", "tool_use"]
      }
    ],
    "executors": [
      { "id": "CLAUDE_CODE", "name": "Claude Code", "description": "..." },
      { "id": "AMP", "name": "AMP", "description": "..." }
    ],
    "agents": [
      { "id": "default", "name": "Default Agent" },
      { "id": "planning", "name": "Planning Agent" }
    ],
    "permission_policies": [
      { "id": "read_only", "name": "Read Only" },
      { "id": "allow_writes", "name": "Allow Writes" }
    ]
  }
}
```

### 2) 前端类型定义

参考 vibe-kanban 的 `shared/types.ts`：

```typescript
export interface ExecutorConfig {
  executor: string;           // 执行器 ID（如 "CLAUDE_CODE"）
  variant?: string;           // 预设变体名称
  modelId?: string;          // 模型 ID 覆盖
  agentId?: string;          // 代理模式 ID
  reasoningId?: string;      // 推理强度 ID
  permissionPolicy?: string; // 权限策略 ID
}

export interface ModelSelectorConfig {
  providers: ModelProvider[];
  models: ModelInfo[];
  executors: ExecutorInfo[];
  agents: AgentInfo[];
  permissionPolicies: PermissionPolicyInfo[];
}

export interface ModelInfo {
  id: string;
  name: string;
  provider: string;
  description?: string;
  capabilities?: string[];
}
```

### 3) 配置持久化

- 使用 `localStorage` 存储用户最后选择的配置
- Key: `agentdash:executor-config`
- 在组件加载时读取并作为默认值

---

## 用户体验（UX）设计要点

### 模型选择器交互流程

```
┌─────────────────────────────────────────────────────┐
│ SessionPage                                         │
│ ┌─────────────────────────────────────────────────┐ │
│ │ 模型选择器按钮（显示当前选择）                  │ │
│ │ [Claude Code] [claude-3-5-sonnet] ▼             │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
│ 点击后弹出：                                        │
│ ┌─────────────────────────────────────────────────┐ │
│ │ ┌─────────────┬───────────────────────────────┐ │ │
│ │ │ 执行器      │ 模型列表                      │ │ │
│ │ │ ──────────  │ ───────────────────────────── │ │ │
│ │ │ • Claude    │ 按提供商分组：                │ │ │
│ │ │   Code  ✓   │                               │ │ │
│ │ │ • AMP       │ Anthropic                     │ │ │
│ │ │ • Gemini    │   ○ claude-3-opus             │ │ │
│ │ │             │   ● claude-3-5-sonnet  ✓      │ │ │
│ │ │             │   ○ claude-3-haiku            │ │ │
│ │ │             │                               │ │ │
│ │ │             │ Google                        │ │ │
│ │ │             │   ○ gemini-1.5-pro            │ │ │
│ │ └─────────────┴───────────────────────────────┘ │ │
│ │                                                 │ │
│ │ 高级选项（可折叠）：                            │ │
│ │ ┌─────────────────────────────────────────────┐ │ │
│ │ │ Variant: [Default ▼]                        │ │ │
│ │ │ Agent Mode: [Default ▼]                     │ │ │
│ │ │ Permission: [Allow Writes ▼]                │ │ │
│ │ └─────────────────────────────────────────────┘ │ │
│ └─────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
```

### 状态展示

- **加载中**：显示 spinner，按钮禁用
- **加载失败**：显示错误提示，提供重试按钮
- **连接断开**：显示离线状态，自动重连

---

## 需求（Requirements）

### R1: 后端 - 实现 WebSocket 发现端点

- [ ] 在 `agentdash-api` 中新增 `/api/agents/discovered-options/ws` WebSocket 处理器
- [ ] 从 vibe-kanban executor 系统读取可用执行器列表
- [ ] 从 vibe-kanban 配置读取可用模型列表
- [ ] 实现心跳机制和错误处理
- [ ] 支持客户端主动刷新请求

### R2: 前端 - 创建核心组件和 Hooks

- [ ] 创建 `useExecutorDiscovery` Hook（WebSocket 连接、状态管理、重连）
- [ ] 创建 `useExecutorConfig` Hook（配置管理、持久化、验证）
- [ ] 创建 `ModelSelector` 组件（按钮 + 弹出面板）
- [ ] 创建 `ExecutorList` 子组件（执行器列表）
- [ ] 创建 `ModelList` 子组件（按提供商分组的模型列表）
- [ ] 创建 `AdvancedOptions` 子组件（变体、代理、权限）

### R3: 前端 - 集成到 SessionPage

- [ ] 替换现有的 `executor`/`modelId` 文本输入框
- [ ] 使用 `ModelSelector` 组件
- [ ] 保持与 `promptSession` API 的兼容性
- [ ] 添加配置重置按钮（恢复默认）

### R4: 状态管理和错误处理

- [ ] 实现 `localStorage` 持久化
- [ ] 处理 WebSocket 连接失败（显示错误信息）
- [ ] 处理后端返回空列表的情况
- [ ] 处理配置无效的情况（如选择的模型在当前执行器不可用）

---

## 验收标准（Acceptance Criteria）

- [ ] 后端 WebSocket 端点 `/api/agents/discovered-options/ws` 可以返回执行器/模型列表
- [ ] 前端 `ModelSelector` 组件可以正确展示所有选项
- [ ] 用户选择配置后，点击发送 prompt 会使用正确的 `executorConfig`
- [ ] 刷新页面后，上次选择的配置会被恢复
- [ ] 网络断开后，前端能自动重连并恢复选择器状态
- [ ] 选择器支持键盘导航和屏幕阅读器（基础无障碍支持）

---

## 技术方案草案（Technical Notes）

### 实现路径（分阶段）

#### Phase 1: 后端基础 API
1. 在 `crates/agentdash-api/src/` 创建 `discovery/` 模块
2. 实现 `DiscoveredOptionsWebSocket` 处理器
3. 从 vibe-kanban executor 系统桥接配置数据
4. 添加路由注册

#### Phase 2: 前端 Hooks
1. 创建 `useExecutorDiscovery.ts`（参考 vibe-kanban 实现）
2. 创建 `useExecutorConfig.ts`（配置管理 + localStorage 持久化）
3. 添加类型定义到 `frontend/src/types/executor.ts`

#### Phase 3: 前端 UI 组件
1. 创建 `ModelSelector/` 组件目录
2. 实现 `ModelSelector.tsx`（主容器）
3. 实现子组件：`ExecutorList.tsx`、`ModelList.tsx`、`AdvancedOptions.tsx`
4. 添加样式（使用项目现有的 Tailwind/shadcn 风格）

#### Phase 4: 集成与测试
1. 在 `SessionPage.tsx` 中替换现有输入
2. 端到端测试：选择配置 → 发送 prompt → 验证请求体
3. 边界测试：网络断开、后端返回错误、空列表

### 参考代码位置

| 组件 | 参考来源 |
|------|----------|
| `useExecutorDiscovery` | `third_party/vibe-kanban/packages/web-core/src/shared/hooks/useExecutorDiscovery.ts` |
| `useExecutorConfig` | `third_party/vibe-kanban/packages/web-core/src/shared/hooks/useExecutorConfig.ts` |
| `ModelSelectorPopover` | `third_party/vibe-kanban/packages/ui/src/components/ModelSelectorPopover.tsx` |
| `ModelList` | `third_party/vibe-kanban/packages/ui/src/components/ModelList.tsx` |
| 类型定义 | `third_party/vibe-kanban/shared/types.ts` |

### 风险与注意事项

1. **跨版本兼容性**：vibe-kanban 的 executor 配置格式可能与 AgentDashboard 不完全一致，需要适配层
2. **性能**：模型列表可能很大，需要虚拟滚动或分页（vibe-kanban 使用了 `react-window`）
3. **状态同步**：如果后端配置变化，需要及时推送到前端
4. **类型一致性**：确保前端类型与后端 Rust 类型保持一致

---

## 相关任务

- 前置任务：[02-26-frontend-agent-session-mvp](archive/2026-02/02-26-frontend-agent-session-mvp/)（Session 视图 MVP 已完成）
- 后续任务：多后端执行器隔离（按 backendId 路由不同执行器）
