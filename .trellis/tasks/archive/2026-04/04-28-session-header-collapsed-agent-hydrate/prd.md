# Session 配置条默认收起 + Agent Defaults 自动加载

## Goal

把 [ExecutorSelector](../../../frontend/src/features/executor-selector/ui/ExecutorSelector.tsx) 从"永远完整展示三行 + 固定硬编码默认值"改造成：

1. **默认只显示胶囊行**，浮在对话框上面；胶囊行右侧加一个展开按钮，点击才显示完整的执行器 / 模型 / 推理级别 / 高级 / 重置配置
2. **默认值从当前 session 绑定的 agent 自动加载**，而不是一上来就是 `PI_AGENT + 选择模型...`。新建 session 时同样读 agent 默认配置

## 背景（调研结论）

详见 `.trellis/workspace/claude-agent/research/session-header-redesign.md`。要点：

- UI：[ExecutorSelector.tsx](../../../frontend/src/features/executor-selector/ui/ExecutorSelector.tsx) 主行 + 选中模型信息 + 胶囊行**永远同时渲染**，只有"高级"面板是局部展开
- 默认值：[useExecutorConfig.ts:8-11](../../../frontend/src/features/executor-selector/model/useExecutorConfig.ts) 硬编码 `PI_AGENT + medium`，只 fallback 到 localStorage，**完全不看 session 绑定的 agent**
- 截图场景（"选择模型..."）：[agent-tab-view.tsx:250](../../../frontend/src/features/agent/agent-tab-view.tsx) 没传 `executorHint`，hydrate 链路断
- 后端数据已备好：`GET /sessions/{id}/context` 返回 `TaskSessionExecutorSummary`（服务端 merge 后真值），前端拉到了但只用于展示，**未回填到 useExecutorConfig**

## Requirements

### UI 改造

- [ ] `ExecutorSelector` 新增外层 `collapsed` state，**默认 `true`**
- [ ] 收起态：只渲染胶囊行（`ConfigTag` 组） + 右侧一个展开按钮（▲/▼ 旋转箭头，复用 `ContextPanelShell` 的 `rotate-90` 样式）
- [ ] 展开态：渲染完整主行（执行器 / 模型 / 推理级别 + 高级 + 重置） + 选中模型信息 + 胶囊行 + 高级面板
- [ ] 胶囊行垂直节奏要瘦（视觉上"浮在"对话框上方，不占据大块空间）
- [ ] `defaultCollapsed` prop 可覆盖（agent-preset-editor 等嵌入场景保留强制展开的能力）

### 数据 Hydrate

- [ ] `useExecutorConfig` 新增 `hydrate(source)` 方法与 `initialSource` 参数
- [ ] 加载优先级：**session 服务端真值 > agent defaults > localStorage > 硬编码**
- [ ] 新增原子写入 API `setAll(partial)`，避免 `setExecutor` 副作用清空 `modelId / thinkingLevel`
- [ ] 引入 `sessionIdSeenRef`：只在 `sessionId` 变更的首帧 hydrate；用户手改后不再被覆盖

### 调用方接入

- [ ] `SessionChatView` 新增 `agentDefaults?: ProjectAgentExecutor | TaskSessionExecutorSummary` prop
- [ ] `SessionChatView` 内 `useEffect` 依赖 `[sessionId, agentDefaults]` 调用 `hydrate`
- [ ] [agent-tab-view.tsx:250](../../../frontend/src/features/agent/agent-tab-view.tsx) 从 `agentsByProjectId` 取当前 agent 的 `executor` 传入 `agentDefaults`
- [ ] [SessionPage.tsx:617](../../../frontend/src/pages/SessionPage.tsx) 优先用 `sessionContextSnapshot?.executor`，回退 `taskAgentBinding` / `projectAgentContext`
- [ ] `story-session-panel` / `task-agent-session-panel` 保持现有行为（前者按需传，后者仍 `showExecutorSelector={false}`）

## Acceptance Criteria

- [ ] 从项目 tab 进入一个"配了 gpt-5 / high"的 agent 的会话：
  - 默认只见胶囊行（"● pi_agent"、"provider: ..."、"model: gpt-5"、"thinking: high"）
  - 点击展开后三个下拉已预选正确值，不是"选择模型..."
- [ ] 用户在当前 session 手动切到别的模型后，切走再切回来**不会被 hydrate 覆盖**
- [ ] 切到另一个绑定了不同 agent 的 session，配置自动切换为新 agent 的默认
- [ ] `/session/:id` 独立页场景也生效（优先用 `sessionContextSnapshot.executor`）
- [ ] `agent-preset-editor` 嵌入场景仍保持展开
- [ ] typecheck / lint 通过

## Definition of Done

- 改动限于 frontend/
- 手动验证上述 AC 场景
- 无 TypeScript / ESLint 报错
- 无需后端改动（数据已备好）

## Out of Scope

- 后端新增 session-level executor override 持久化接口（调研 §6.2 Step 5，留作后续）
- localStorage 的 session 分键存储（维持"最近一次用户手选值"语义）
- `task-agent-session-panel` 由 hidden 改为"默认收起"（保持现状，后续统一）
- 胶囊行的 loading / skeleton 态（优先基础流程，若有明显抖动再加）

## Technical Approach

### Step 1: useExecutorConfig 扩展

```ts
type ExecutorSource = Partial<PersistedExecutorConfig>;

export function useExecutorConfig(options?: {
  initialSource?: ExecutorSource;
  sessionId?: string;  // 用于 dirty tracking
}) {
  // 维持现有 executor/providerId/modelId/thinkingLevel/permissionPolicy state
  // 新增 hydrate(source: ExecutorSource) - 原子 setAll，绕过 setExecutor 的副作用
  // 内部 sessionIdSeenRef，仅 sessionId 切换首帧执行 hydrate
}
```

优先级：`initialSource` 非空 → 用它；否则走现有 localStorage → 硬编码逻辑。

### Step 2: ExecutorSelector 外层收起

新增：

```tsx
const [collapsed, setCollapsed] = useState(defaultCollapsed ?? true);
```

收起时返回：

```tsx
<div className="flex items-center gap-2 px-1 py-1.5">
  <ConfigTagRow ... />
  <button onClick={() => setCollapsed(false)}>▲</button>
</div>
```

展开时：现有主行 + 选中模型信息 + 胶囊 + 高级面板 + "收起"按钮（▼）。

### Step 3: SessionChatView 接入

```tsx
const execConfig = useExecutorConfig({ sessionId, initialSource: agentDefaults });
```

### Step 4: 调用方透传

- `agent-tab-view`: 从 `agentsByProjectId[currentProjectId]` 根据 `selectedSessionId` 反查 agent → 把 `agent.executor` 传下去。也可直接用 `sessionContextSnapshot.executor`（若已拉到）
- `SessionPage`: 用 `sessionContextSnapshot?.executor ?? buildFromTaskBinding()`

## Decision (ADR-lite)

**Context**: 原实现把 executor 配置当作"用户全局偏好"用 localStorage 托管，和 agent 绑定、session 绑定完全脱钩；UI 又永远展开三行，视觉极重。

**Decision**:
1. UI 默认收起，只留胶囊 + 展开按钮（Approach A：外层 collapsed state，复用现有 rotate-90 箭头样式）
2. 引入 hydrate 机制让 agent / session 真值主导默认值，localStorage 降级为"无真值时的记忆"

**Consequences**:
- 用户手改本地覆盖 vs agent 真值回填的边界靠 `sessionIdSeenRef` 卡；跨 session 切换瞬间会丢掉"本 session 未持久化的手改"，但已持久化的 session 级 override 本就不在当前架构里，本次不做
- localStorage 保留，避免"新建首个 session 时还要等网络"的等待感
- task-agent-session-panel 继续 hidden；后续若要统一为"收起"，改动极小

## Technical Notes

- 调研报告：`.trellis/workspace/claude-agent/research/session-header-redesign.md`
- 关键文件：
  - [frontend/src/features/executor-selector/ui/ExecutorSelector.tsx](../../../frontend/src/features/executor-selector/ui/ExecutorSelector.tsx)
  - [frontend/src/features/executor-selector/model/useExecutorConfig.ts](../../../frontend/src/features/executor-selector/model/useExecutorConfig.ts)
  - [frontend/src/features/acp-session/ui/SessionChatView.tsx](../../../frontend/src/features/acp-session/ui/SessionChatView.tsx)
  - [frontend/src/features/agent/agent-tab-view.tsx](../../../frontend/src/features/agent/agent-tab-view.tsx)
  - [frontend/src/pages/SessionPage.tsx](../../../frontend/src/pages/SessionPage.tsx)
  - [frontend/src/features/session-context/context-panels.tsx](../../../frontend/src/features/session-context/context-panels.tsx)（复用 `rotate-90` 箭头样式参考）
- 类型：`ProjectAgentExecutor` (`types/index.ts:255`) / `TaskSessionExecutorSummary` (`types/context.ts:140`)
