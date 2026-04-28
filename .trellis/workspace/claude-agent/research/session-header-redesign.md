# Research: Session 前端配置 UI（执行器 / 模型 / 推理级别）改造调研

- **Query**: 找到配置条的组件、默认值来源、agent 配置结构，并给出收起/展开改造切入点
- **Scope**: 内部（前端 + 部分后端契约）
- **Date**: 2026-04-28

---

## 1. UI 组件定位

### 配置条本体

**文件**: `frontend/src/features/executor-selector/ui/ExecutorSelector.tsx`

- 组件名 `ExecutorSelector`（默认 export: 第 67 行；props 定义第 5–30 行）
- 这是唯一渲染"执行器 / 模型 / 推理级别 / 高级 / 重置"这一行的组件
- 关键关键字位置：
  - "执行器" label：`ExecutorSelector.tsx:178`
  - "选择执行器…" 占位：`ExecutorSelector.tsx:187`
  - "模型" label：`ExecutorSelector.tsx:201`
  - "选择模型…" 占位（图中就是这个）：`ExecutorSelector.tsx:220`
  - "推理级别" label：`ExecutorSelector.tsx:245`
  - "高级" / "收起" 按钮（已有 `showAdvanced` 本地 state）：`ExecutorSelector.tsx:88, 264–271`
  - "重置" 按钮：`ExecutorSelector.tsx:272–279`
  - 下方胶囊区（"● Pi Agent"、"thinking: medium"、"provider: xxx"、"model: xxx"）：`ExecutorSelector.tsx:306–319`，组件 `ConfigTag`（第 381–394）
  - "高级选项面板"（provider / 手动 model id / 权限策略）：`ExecutorSelector.tsx:321–376`
- 容器样式 `ExecutorSelector.tsx:172`：`rounded-[14px] border bg-secondary/45 p-3.5`

### 谁渲染它

**文件**: `frontend/src/features/acp-session/ui/SessionChatView.tsx`

- `SessionChatView` 是整块聊天视图，它在输入框上方渲染 `ExecutorSelector`：`SessionChatView.tsx:750–772`
- 由 `showExecutorSelector` prop（默认 true）控制：`SessionChatView.tsx:212, 266, 750`
- Hook 调用链（状态托管）：
  - `useExecutorDiscovery()`：`SessionChatView.tsx:335`（取所有 executor 列表）
  - `useExecutorConfig()`：`SessionChatView.tsx:336`（当前选择的持久化状态）
  - `useExecutorDiscoveredOptions(executor)`：`SessionChatView.tsx:337`（拉该 executor 的 providers/models 枚举）
  - hint 应用：`SessionChatView.tsx:343–354`（把 `executorHint` 映射成 `execConfig.setExecutor(id)`）
- 输入区整体 `SessionChatView.tsx:728–840`；`ExecutorSelector` 在 "富文本输入" 框上方，富文本块在 `SessionChatView.tsx:775–836`

### SessionChatView 的调用方（需要测试的所有入口）

| 调用方 | 文件:行 | `showExecutorSelector` | `executorHint` 传入 |
|---|---|---|---|
| `SessionPage`（/session/:id 独立页，含新建） | `frontend/src/pages/SessionPage.tsx:617–627` | 默认 true | `executorHint` 来自 task binding / project agent context / session snapshot（`SessionPage.tsx:325–328`） |
| `agent-tab-view`（项目侧边栏聊天） | `frontend/src/features/agent/agent-tab-view.tsx:250–257` | `true`（显式） | **未传** |
| `task-agent-session-panel`（任务执行面板） | `frontend/src/features/task/task-agent-session-panel.tsx:298–311` | `false`（隐藏整个选择器） | — |
| `story-session-panel` | `frontend/src/features/story/story-session-panel.tsx:222` | 需确认，默认 true | 需确认 |

**截图中"选择模型…"就是 agent-tab-view 的场景** —— 它没传 `executorHint`，导致即使该 agent 本身已有 `executor.model_id / thinking_level`，`SessionChatView` 也完全不知道。

---

## 2. 配置来源

### 2.1 "Pi Agent"、"thinking: medium" 这些默认值

**文件**: `frontend/src/features/executor-selector/model/useExecutorConfig.ts`

硬编码在 Hook 顶部：

- `DEFAULT_EXECUTOR = "PI_AGENT"`：第 11 行
- `DEFAULT_THINKING_LEVEL = "medium"`：第 8 行
- 加载顺序 `loadOrDefault`（第 54–61）：**先读 `localStorage`，读不到才 fallback 到硬编码默认值**
- localStorage key：`agentdash:executor-config-v2`（第 5 行）
- 持久化字段：`PersistedExecutorConfig = { executor, providerId, modelId, thinkingLevel, permissionPolicy }`（见 `types.ts`）

结论：**默认值完全和 agent 绑定无关**。即使 session 已绑定一个设置为 GPT-5 / high 的 agent，UI 仍然先读 localStorage；localStorage 为空则落到 PI_AGENT + medium + 空 model。

### 2.2 新建 session 如何初始化

路径：`handleOpenAgent`（agent tab）或 `handleCreateSession`（session page）

- **`agent-tab-view.tsx:136–148`** 调用 `openProjectAgentSession(projectId, agentKey)`：
  - Store 实现：`frontend/src/stores/projectStore.ts:491–517`
  - 后端接口：`POST /projects/{id}/agent-links/{agentKey}/session`
  - 返回 `OpenProjectAgentSessionResult`（`types/index.ts:281–286`），内含新的 `session_id` 和完整 `ProjectAgentSummary`（含 `executor: ProjectAgentExecutor`，即 `executor/provider_id/model_id/thinking_level/permission_policy`）
  - **返回的 agent.executor 在这里就被丢弃了**，只用 `session_id` 导航到右侧 `SessionChatView`。UI 层 `useExecutorConfig` 仍走 localStorage。

- **`SessionPage.tsx`** 场景稍好：
  - `executorHint = taskAgentBinding?.agent_type ?? projectAgentContext?.executor_hint ?? taskExecutorSummary?.executor`（`SessionPage.tsx:325–328`）
  - 但 `executorHint` 只会改 **executor** 字段（`SessionChatView.tsx:348–354` → `setExecutor`），而 `setExecutor` 又会把 `modelId / thinkingLevel` 重置成空 + medium（`useExecutorConfig.ts:130–146`），**仍然丢失了 agent 的 model/provider 偏好**。

### 2.3 后端 session 创建接口是否返回 agent 默认配置

后端已经准备好数据，前端没用：

- `POST /projects/{id}/agent-links/{key}/session` 响应体包含 `agent.executor`（`crates/agentdash-api/src/routes/project_agents.rs:64–94, 398–404`）
  - 返回字段：`executor`、`provider_id`、`model_id`、`agent_id`、`thinking_level`、`permission_policy`
  - 后端通过 `executor_config_from_agent_config(agent_type, merged_config)` 解析（第 866、910 行）

- `GET /sessions/{id}/context` 响应体 `SessionContextSnapshot.executor: TaskSessionExecutorSummary`（前端类型 `frontend/src/types/context.ts:140–151, 175–180`）
  - 同样包含 `executor/provider_id/model_id/thinking_level/permission_policy/preset_name/source`
  - 这是 **session 已存在时** 可用的"服务端解析后真值"
  - 前端已经调用了该接口（`frontend/src/services/session.ts:187–203`）并缓存到 `sessionContextSnapshot`，但只用于展示（`SessionPage.tsx:259`、`context-panels.tsx:220–230`、`:727–739`），没有回填到 `useExecutorConfig`

### 2.4 session ↔ agent 的绑定关系

- `SessionContextPayload.agent_binding: AgentBinding | null`（`types/index.ts:379–387`、`services/session.ts:152–175`）
  - 字段：`agent_type / agent_pid / preset_name / prompt_template / initial_context / thinking_level / context_sources`
  - 注意 **`AgentBinding` 没有 `model_id` / `provider_id` / `permission_policy`**；完整模型配置仅存在于 `SessionContextSnapshot.executor`
- Task 级：`Task.agent_binding`（`types/index.ts:398`）
- Project 级：`ProjectAgentLink.merged_config`（`types/index.ts:196–211`）—— 后端已经 merge 好的配置
- Agent 实体：`AgentEntity.base_config`（`types/index.ts:187–193`）

---

## 3. 展开/收起状态现状

### ExecutorSelector 内部已有"局部展开"

`ExecutorSelector.tsx:88`：`const [showAdvanced, setShowAdvanced] = useState(false);`

但这是"**高级**"按钮控制的**内层**展开（provider / 手动 model id / 权限策略），**不是整个条的展开/收起**。目前：

- 主行（执行器 / 模型 / 推理级别 / 高级 / 重置）**始终显示**
- 下方胶囊（ConfigTag 行）**只要 executor 非空就始终显示**（`ExecutorSelector.tsx:306`）

所以当前无"只显示胶囊 / 点击展开主行"的交互。

### 项目中可复用的展开/收起样式

1. **`ContextPanelShell`** —— `frontend/src/features/session-context/context-panels.tsx:147–203`
   - 整行按钮 + SVG 箭头（`rotate-90` 动画 `:172`）+ 下方面板条件渲染（`isOpen &&` `:196`）
   - Props：`{ title, subtitle, badges, isOpen, onToggle, children }`
   - 视觉与现有 UI 最接近，**强烈推荐复用这个箭头旋转模式**

2. **`ContextSourcesSummary`** —— `frontend/src/features/task/task-agent-session-panel.tsx:54–85`
   - 同款 `rotate-90` 箭头 + `setExpanded((v) => !v)`；更轻量

3. **`SessionHistoryPanel`** —— `frontend/src/features/project/project-agent-view.tsx:70–140`
   - 同款 `setExpanded / expanded &&`，风格略不同

---

## 4. Agent 配置结构

### 4.1 Agent 实体字段（能对应 UI 三个下拉的）

`ProjectAgentExecutor`（`frontend/src/types/index.ts:255–262`）—— 这是前端拿到的"解析后"结构：

```typescript
{
  executor: string;                 // → UI "执行器" 下拉
  provider_id?: string | null;      // → UI "高级→模型提供方"
  model_id?: string | null;         // → UI "模型" 下拉
  agent_id?: string | null;         // （agent 侧进程唯一 id，不是 UI 配置）
  thinking_level?: ThinkingLevel | null;  // → UI "推理级别" 下拉
  permission_policy?: string | null;      // → UI "高级→权限策略"
}
```

挂载在 `ProjectAgentSummary.executor`（`types/index.ts:271–279`）。

### 4.2 是否有 `default_model` / `default_reasoning` 字段

**没有独立的 "default_*" 字段**。字段命名就是 `provider_id / model_id / thinking_level` 本身，它们 **就是** "默认值"——即配置在 agent 上的那份值。

写入/读出路径：
- 存在 `AgentEntity.base_config: Record<string, unknown>`（动态 JSON，`types/index.ts:187–193`）
- 经 `ProjectAgentLink.merged_config`（`:196–211`）合并后端 override
- 后端 `executor_config_from_agent_config`（`crates/agentdash-api/src/routes/project_agents.rs:910–928`）从 merged config 提取成结构化 `ProjectAgentExecutorResponse`

### 4.3 前端拿 agent 详情的路径

- **Store**：`useProjectStore` → `agentsByProjectId`（`frontend/src/stores/projectStore.ts:21, 72, 476`）
- **加载**：`fetchProjectAgents(projectId)` → `GET /projects/{id}/agents`
- **映射**：`mapProjectAgentSummary`（`projectStore.ts:81`）
- **读用处**：`agents: ProjectAgentSummary[] = agentsByProjectId[currentProjectId]`（`agent-tab-view.tsx:65` 左右）

此外：
- `openProjectAgentSession` 响应直接带上 `agent`（含 executor），但当前调用方 `agent-tab-view.tsx:139` 没用这个数据
- `GET /sessions/{id}/context` → `TaskSessionExecutorSummary`（已 merge 好的服务端真值）

---

## 5. 改动影响面

### 5.1 配置条被复用的页面

| 调用点 | 状态 |
|---|---|
| `SessionPage` (`/session/:id`) | 显示（默认） |
| `agent-tab-view`（项目 tab 内聊天右栏） | 显示 |
| `task-agent-session-panel`（任务执行页） | 隐藏（`showExecutorSelector={false}`） |
| `story-session-panel` | 推测显示（默认） |

另外 `ExecutorSelector` 内的 `model_selector` 相关下拉也在 `agent-preset-editor.tsx` 被部分复用，但走的是不同表单（不属于 session header）。

### 5.2 Session-level override vs Agent-level default

存在概念但当前 **UI 层不区分**：

- Agent-level default = `AgentEntity.base_config` + `ProjectAgentLink.config_override` → 后端 merge → `ProjectAgentSummary.executor` / `SessionContextSnapshot.executor`
- Session-level override = **调用 `/sessions/{id}/prompt` 时传的 `executorConfig`**（`services/executor.ts:10–17, 26–31`）
- 前端 `useExecutorConfig` 托管的值**既是默认值，也是每次 prompt 的 override**——它全程由 localStorage 驱动，没有"当前 agent 默认"的概念。

---

## 6. 建议改造切入点

### 6.1 目标拆解

| 需求 | 落脚点 |
|---|---|
| 默认只显示胶囊行 | 改 `ExecutorSelector.tsx` 增加"整条收起"模式 |
| 点击向上展开显示完整配置 | 同上，新增一个外层 `collapsed` state |
| 配置从 session 绑定的 agent **自动加载** | 让 `useExecutorConfig` 支持 "hydrate from source"；在 `SessionChatView` 用 `sessionContextSnapshot.executor` 或 `ProjectAgentSummary.executor` hydrate |
| 新建会话读取 agent 默认 | 同上 hydrate，或在 `openProjectAgentSession` 成功后把 `result.agent.executor` 写回 executor config |

### 6.2 建议的文件改动 + 顺序

**Step 1：拓宽 `useExecutorConfig` 的数据来源**
- 文件：`frontend/src/features/executor-selector/model/useExecutorConfig.ts`
- 新增参数 `initialSource?: { executor?; providerId?; modelId?; thinkingLevel?; permissionPolicy? }`
- 加载优先级：**session 服务端真值 > 显式 initialSource > localStorage > 硬编码默认**
- 新增 `hydrate(source)` 方法，用于 session 切换后回填
- 注意：**当前用户手动改过的值不应被静默覆盖**——建议引入 "dirty flag"，只有 dirty=false 时才接受 hydrate（或仅在 sessionId 变更瞬间接受）

**Step 2：改造 `ExecutorSelector` UI**
- 文件：`frontend/src/features/executor-selector/ui/ExecutorSelector.tsx`
- 新增外层 `collapsed` state（默认 `true`）
- 收起时：只渲染现有的 `ConfigTag` 行（第 306–319），并在右侧加一个"展开"按钮（▲/▼ 箭头）
- 展开时：渲染主行（第 174–281）+ 选中模型信息（第 283–302）+ 胶囊行 + 高级面板
- 可复用 `ContextPanelShell` 的箭头旋转 CSS 风格（`rotate-90` class + 条件渲染）
- 可选 prop：`defaultCollapsed?: boolean`（默认 true，便于 agent-preset-editor 等嵌入场景强制展开）

**Step 3：在 `SessionChatView` 里接入 hydrate**
- 文件：`frontend/src/features/acp-session/ui/SessionChatView.tsx:333–390`
- 新增 prop：`agentDefaults?: ProjectAgentExecutor`（或 `TaskSessionExecutorSummary`）
- 在现有 `useExecutorConfig()` 调用后 `useEffect(() => execConfig.hydrate(agentDefaults), [sessionId, agentDefaults])`
- 现有 `executorHint` hint 应用逻辑（第 348–354）可以升级为**把整个 executor config 而不只是 executor id 注入**

**Step 4：调用方传入 agent defaults**
- `agent-tab-view.tsx:250–257`：
  - 在 `handleOpenAgent` 中保留 `result.agent.executor`，或从 `agentsByProjectId[projectId]` 根据当前 `selectedSessionId` 回查
  - 把它作为 `agentDefaults` 传给 `SessionChatView`
- `SessionPage.tsx:617–627`：
  - 优先用 `sessionContextSnapshot?.executor`（它已经是服务端 merge 后真值）
  - 回退到 `taskAgentBinding` / `projectAgentContext`
- `story-session-panel.tsx` / `task-agent-session-panel.tsx`：按需传入（task 场景甚至可以完全覆盖并隐藏选择器，现状如此）

**Step 5（可选）：向后端追加 session-level override 接口**
- 当前只能通过 `/sessions/{id}/prompt` 的 `executorConfig` 字段带上 override
- 若需要持久化 session 级 override，可考虑扩展 `/sessions/{id}/context` 或新增 `PATCH /sessions/{id}/executor-config`
- **本次改造不建议做**，优先级低

### 6.3 风险点

1. **Hydrate 覆盖用户手改**
   - 用户已经在当前 session 改过模型，新数据不应覆盖
   - 方案：在 `useExecutorConfig` 里维护 `sessionIdSeenRef`，只有 `sessionId` 变更的第一帧才 hydrate；或用户一旦编辑后设 dirty flag，hydrate 跳过

2. **localStorage 全局状态 vs session 本地状态的冲突**
   - 现状 localStorage 是 **跨 session 共享**；若改为 "以 agent default 为准"，localStorage 的语义需要重新定义
   - 建议：localStorage 降级为 "最近一次用户手选值"，仅在 **没有** session/agent default 时使用（新建首次）；已有 session 一律优先 agent default
   - 也可以考虑按 `sessionId` 分键存（空间成本小，最多几百条）

3. **`setExecutor` 会清空 model/thinking**（`useExecutorConfig.ts:130–146`）
   - 新的 hydrate 必须绕开这个逻辑，或提供 `setAll(partial)` 原子 API 一次性写入，避免 race

4. **`ExecutorSelector` 收起后"重置"按钮如何安放**
   - 建议：胶囊行右侧只保留"展开/收起"；重置按钮放进展开后的主行（保持现位），或胶囊右键菜单。设计上"胶囊 = 只读摘要"，不放破坏性动作

5. **`task-agent-session-panel`**
   - 它目前 `showExecutorSelector={false}`；改造完成后建议评估是否也切换到"默认收起"的新模式，把隐藏选择器改为"默认收起、可展开"，提升一致性（改法：删掉 prop 或默认 true）

6. **首渲染抖动**
   - Hydrate 依赖 `sessionContextSnapshot`（异步 fetch），首渲染时仍是 localStorage/硬编码
   - 需给胶囊加 loading/skeleton 态，或在 `SessionChatView` 里等 `sessionContextSnapshot` ready 再渲染 `ExecutorSelector`

### 6.4 最小验证路径

1. 改 `useExecutorConfig` + `ExecutorSelector`（Step 1–2），给 `agent-tab-view` 传 `agentDefaults`（Step 4 局部）
2. 本地跑：项目 tab → 打开一个配置了 GPT-5 / high 的 agent 的会话
3. 预期：
   - 进入时只见胶囊 "● Pi Agent"/"provider: xxx"/"model: gpt-5"/"thinking: high"
   - 点展开后三个下拉已选中正确值
   - 手动切换模型 → 仅本会话生效 → 切到别的会话不串
4. 再推 Step 3（`SessionChatView.agentDefaults` prop）+ Step 4 其他调用方

---

## Caveats / Not Found

- 没有深入 `story-session-panel.tsx` 的 executor 传参情况（截图场景是项目 tab，优先级低）
- 后端 `executor_config_from_agent_config` 只读到了名字，未完整审阅其 merge 规则与 permission_policy 默认值路径
- `agent_id` 字段的语义（是 "agent 进程 id" 还是 "agent entity id"）未在本次调研中核实；改造 hydrate 逻辑时若误用，可能影响 pi_agent 连接器的 agent 复用策略
- 未核查是否有相关的 `.trellis/spec/frontend/*` 明确要求 "配置条必须常驻可见" —— 改造前建议搜一次 spec
