# Lifecycle Designer Redesign — Design

## 目标

把现有 `lifecycle-editor-shell + step-inspector + lifecycle-dag-canvas + workflow contract panels` 重塑为以 Activity 为一等公民的编辑体验，覆盖新模型全部能力。

## 架构总览

```
LifecycleEditorShell (DAG-only)
├── TopBar              校验/保存按钮、状态徽章
├── ValidationOverlay   bottom-left ValidationPanel（error/warn 列表）
├── DAG Canvas          结构 + 端口连线 + 极简节点徽章
└── Right Sidebar (受选中对象驱动)
    ├── ActivityInspector（选中节点时）
    │   ├── Sticky Header（key + entry 切换 + close）
    │   ├── §1 Identity         key / desc / iteration_policy / join_policy
    │   ├── §2 Executor         executor.kind switch + per-kind 表单
    │   ├── §3 Ports & Policy   activity.input/output_ports + completion_policy
    │   └── §4 Workflow Contract（Agent activity 限定，<details> 折叠）
    │       ├── Injection
    │       ├── Capability
    │       ├── HookRules
    │       └── Contract Ports（标准接口，与 §3 ports 通过 mergeContractIntoStep 同步）
    └── TransitionInspector（选中边时，替换 ActivityInspector）
        ├── Sticky Header（from→to + close）
        ├── transition.kind 切换（flow / artifact）
        ├── condition 编辑（按 4 种 kind 切表单）
        ├── artifact_bindings 列表（kind=artifact 时显示）
        └── max_traversals 数字
```

## 数据流

### Store 层（`workflowStore.ts`）

新增字段 / 行为：

```ts
interface LifecycleEditorState {
  draft: LifecycleEditorDraft | null;
  workflowDraftsByActivityKey: Record<string, WorkflowEditorDraft>;
  /** 选中对象（互斥）：activityKey 选中节点；transitionId 选中边；都为 null 显示 lifecycle header */
  selection: { kind: "activity"; activityKey: string }
            | { kind: "transition"; transitionId: string }
            | null;
  // 其余保留：originalId / validation / isSaving / isValidating / dirty / isLoading / error
}
```

把当前 `selectedActivityKey: string | null` 替换为 `selection`，因为新模型中 transition 也是一等编辑对象。

新 store actions：

| action | 语义 |
|---|---|
| `selectLifecycleActivity(key \| null)` | 选中节点 |
| `selectLifecycleTransition(id \| null)` | 选中边 |
| `updateLifecycleEditorActivity(activityKey, patch)` | 已有 |
| `updateActivityWorkflowDraft(activityKey, patch)` | 已有 |
| `updateLifecycleEditorTransition(id, patch)` | **新增**：按 transitionId 定位并替换 |
| `addArtifactBinding(transitionId, binding)` | 新增 |
| `updateArtifactBinding(transitionId, idx, patch)` | 新增 |
| `removeArtifactBinding(transitionId, idx)` | 新增 |
| `setTransitionKind(transitionId, kind)` | 新增。从 artifact 改 flow 时清空 bindings |
| `setActivityCompletionPolicy(activityKey, policy)` | 新增 |
| `setActivityIterationPolicy(activityKey, patch)` | 新增 |
| `setActivityJoinPolicy(activityKey, policy)` | 新增 |
| `setActivityExecutor(activityKey, executor)` | 新增。内部调用 `ensurePolicyForExecutor` 联动调整 completion_policy |

### `transitionId` 派生

后端 `ActivityTransition` 没有 stable id 字段。前端用 `${from}-->${to}#${index}` 派生（同 from-to 多边时按 index 区分）。canvas 已用类似逻辑（`lifecycleEdgeId`），统一抽到 helper。

### Executor × completion_policy 联动

```ts
function ensurePolicyForExecutor(
  current: ActivityCompletionPolicy,
  newKind: ActivityExecutorSpec["kind"],
): { policy: ActivityCompletionPolicy; reset: boolean } {
  const isCompatible = (p: ActivityCompletionPolicy) => {
    if (newKind === "agent") return p.kind !== "human_decision";
    if (newKind === "function") return p.kind === "output_ports" || p.kind === "executor_terminal";
    if (newKind === "human") return p.kind === "human_decision";
  };
  if (isCompatible(current)) return { policy: current, reset: false };
  if (newKind === "human") {
    return { policy: { kind: "human_decision", decision_port: "decision" }, reset: true };
  }
  return { policy: { kind: "executor_terminal" }, reset: true };
}
```

`reset=true` 时 inspector 顶部 toast 提示。

### Port 双层折叠同步

保留现有 `mergeContractIntoStep`（[step-inspector.tsx:214-222](packages/app-web/src/features/workflow/ui/step-inspector.tsx#L214-L222)）。新 inspector 的 §3 Ports 编辑直接写 `activity.input_ports / output_ports`；§4 Workflow Contract 子区的 contract.ports 编辑通过 `handleOutputPortsChange / handleInputPortsChange` 触发 contract→activity 同步。语义不变，只是 UI 编排改变。

## 组件拆分

| 文件 | 责任 | 改动类型 |
|---|---|---|
| `features/workflow/lifecycle-editor-shell.tsx` | shell 容器、selection 路由、save/validate 按钮 | 简化（移除 Form/DAG mode 判定与 sticky_dag、移除 FormLayout 分支） |
| `features/workflow/ui/lifecycle-dag-canvas.tsx` | DAG 画布、端口连线、节点选择、边选择 | 中改（节点点击 → selection.activity；边点击 → selection.transition；移除 onAddStep 改 onAddActivity 已完成；保留多端口连线创建 binding 已完成） |
| `features/workflow/ui/dag-node.tsx` | 节点视觉 | **重写**（极简徽章 + tooltip） |
| `features/workflow/ui/activity-inspector.tsx` | **新增** Activity 三段 + Contract 折叠 | 新建（取代 step-inspector.tsx） |
| `features/workflow/ui/transition-inspector.tsx` | **新增** transition 编辑 | 新建（取代当前 lifecycle-editor-shell.tsx 内的 TransitionPanel） |
| `features/workflow/ui/sections/IdentitySection.tsx` | §1 Identity + iteration/join policy | 新建 |
| `features/workflow/ui/sections/ExecutorSection.tsx` | §2 Executor 表单（agent/function-api/function-bash/human） | 新建（吸收 step-inspector 的 ExecutorEditor 并扩展 api_request） |
| `features/workflow/ui/sections/PortsAndPolicySection.tsx` | §3 ports + completion_policy | 新建 |
| `features/workflow/ui/sections/WorkflowContractSection.tsx` | §4 折叠的 contract subsection | 新建（包装现有 4 个 panel） |
| `features/workflow/ui/conditions/*.tsx` | 4 种 condition 表单（always / artifact_field_equals / human_decision_equals / agent_signal_equals） | 新建 |
| `features/workflow/ui/ArtifactBindingsEditor.tsx` | bindings 列表编辑 | 新建 |
| `features/workflow/ui/policy/*` | iteration_policy / join_policy / completion_policy 子表单 | 新建 |
| `features/workflow/ui/panels/*` （Injection/Capability/HookRules/Ports） | 现有 contract panel | 复用，仅 props 包装 |
| `features/workflow/ui/step-inspector.tsx` | 旧 inspector | **删除** |
| `features/workflow/ui/step-inspector.test.tsx` | 旧 inspector 测试 | **删除并新写 activity-inspector.test.tsx** |
| `features/workflow/lifecycle-editor-shell.test.tsx` | mode judgement 测试 | **删除 mode 判定测试**（无 Form/DAG 双模），保留 store 集成测试并扩展 selection 用例 |

## 关键 UI 行为契约

### 1. 选中切换
- 点击 canvas 节点 → `selection: { kind: "activity", activityKey }`
- 点击 canvas 边 → `selection: { kind: "transition", transitionId }`
- 点击 canvas 空白 → `selection: null`，sidebar 显示 LifecycleHeader（顶层信息）

### 2. ActivityInspector §1 Identity
- key 输入（rename 触发 store 全图同步）
- description textarea
- isEntry 切换按钮（仅当不是 entry 时显示）
- iteration_policy:
  - max_attempts: 数字输入 + "无限" 复选框（勾选时设为 null）
  - artifact_alias: select(latest / per_attempt / latest_and_history)
- join_policy: select(all / any / first / n_of_m)；选 n_of_m 时旁边显示 n 数字输入
- 删除按钮（footer）

### 3. ActivityInspector §2 Executor
顶部 select(agent / human / function)。entry activity 时 function 选项 disabled。
- agent 表单：workflow_key 输入 + session_policy select
- human 表单：title 输入 + form_schema_key 输入
- function 表单：先 select function.type(api_request / bash_exec)
  - api_request: method select + url_template + body_template（JSON textarea，初版可不做语法高亮）
  - bash_exec: command + args（空格分隔字符串） + working_directory（可空）

切换 executor.kind 触发 `setActivityExecutor` → 内部 `ensurePolicyForExecutor`。如果 reset，inspector 顶部出现一行轻提示「completion_policy 已自动改为 X 因为 …」，3 秒后淡出或用户点 ✕ 关闭。

### 4. ActivityInspector §3 Ports & Policy
- input_ports / output_ports 列表（沿用现有 `OutputPortItem / InputPortItem`，统一 activity 级编辑）
- completion_policy 编辑器：select(output_ports / executor_terminal / human_decision / hook_gate / open_ended)
  - output_ports：multi-select 已声明的 output_ports.key（required_ports 数组）
  - executor_terminal：无字段
  - human_decision：input decision_port（默认 "decision"），下拉提示已声明的 output_ports
  - hook_gate：input hook_key（纯字符串）
  - open_ended：无字段

### 5. ActivityInspector §4 Workflow Contract（Agent only）
`<details>` 元素，summary 文本 "Workflow Contract（资产标准接口）"，默认折叠。展开后渲染：
- InjectionPanel
- CapabilityPanel
- HookRulesPanel
- PortsPanel（contract ports）—— 同步规则不变

### 6. TransitionInspector
- header: `${from} → ${to}`
- transition.kind switch（flow / artifact）：从 artifact 改 flow 时弹 confirm 或直接清空 bindings + toast
- condition 编辑器：select(always / artifact_field_equals / human_decision_equals / agent_signal_equals)
  - always: 无字段
  - artifact_field_equals: activity select（lifecycle 内任意 activity）+ port select（该 activity.output_ports）+ path 输入（JSON path 字符串）+ value 输入（JSON 文本）
  - human_decision_equals: activity select + decision_port 输入 + value 输入（默认下拉 approved/rejected）
  - agent_signal_equals: activity select + signal_key 输入（即 output port key）+ value 输入
- max_traversals: 数字输入 + "无限" 复选框
- ArtifactBindingsEditor（kind=artifact 时显示）：
  - 列表：每行 from_activity select / from_port select / to_port select / alias select / 删除
  - "+ 添加 binding" 按钮

### 7. DAG Canvas 节点视觉
节点 div 结构：
```
[ringIfEntry][validationDot]
  ┌─────────────────────────────┐
  │ {executorIcon} key {policyBadge} │
  │ description (truncate)       │
  │ ┌─ ports handles ─┐          │
  └─────────────────────────────┘
```
徽章颜色：
- agent: indigo
- human: amber
- function: emerald
- completion_policy badge: 同 executor 同色系，但稍浅
- entry ring: ring-primary
- validation dot: red bg + 数字

Tooltip（hover 200ms）：使用现有项目内已有的 tooltip 模式或 react-tooltip 类（先调研项目里是否已有 tooltip 组件，没有就用 native title 临时简化，后续再统一）。

### 8. Validation 反馈
- 校验 issue 列表：`WorkflowValidationResult.issues`
- 三处呈现：
  1. 节点角标（issue 关联到 activity_key 时）
  2. Inspector 顶部（issue 关联到当前 selection 时）
  3. ValidationOverlay（全局列表）
- 校验时机：手动点"校验"按钮 + 保存前自动校验

## 兼容性 & 数据迁移

### 现有 lifecycle 数据
- 已存的 lifecycle definition `iteration_policy` / `join_policy` 字段在 mapper 中已有，加载时 fallback 默认值（max_attempts=1, alias=latest, join=all）
- 不需要后端 schema 变更或迁移
- localStorage `agentdash:editor-dag-sticky:*` 桶可以保留但不再读写（无害遗留）；position 桶 `agentdash:dag-positions:*` 仍使用

### 路由
- `LifecycleEditorShellPage.tsx` 当前 search param `step` / `activity` 双向 fallback（已在 cleanup 任务中改为 activity 优先 + step 兼容）。本任务保留兼容，只读使用 `activity`。

## 风险 & 缓解

| 风险 | 概率 | 缓解 |
|---|---|---|
| 取消 Form 模式后老用户不适应 | 中 | 单 activity 时画布只画 1 节点 + sidebar 完整 inspector，体验上与 Form 接近 |
| condition 4 种 + completion_policy 5 种 + executor 4 子类 = 表单组合爆炸 | 中 | 拆细组件 + 每个组合单测 + Storybook-style 渲染测试覆盖 |
| artifact_bindings 多 binding 时 UI 复杂 | 低 | 默认 1 binding，"添加 binding" 隐藏在每条 transition 的次要按钮 |
| Validation 三处冗余呈现导致 UI 噪音 | 低 | 节点角标只显数字、inspector 只显当前 selection 相关、global panel 在 bottom-left 半透明 |
| Tooltip 实现工作量 | 低 | 初版用 native title；如时间充裕换 radix-ui Tooltip |

## Rollout

- 单 PR 合并（任务范围内）；不开 feature flag（编辑器是直接路由进入的，新旧不能并存）
- 旧 step-inspector.tsx + 测试同 PR 删除
- spec 更新 frontend/workflow-activity-lifecycle.md：新增 "Lifecycle Designer 信息架构" Scenario

## 验证门

- `pnpm --filter app-web typecheck`
- `pnpm --filter app-web lint`
- `pnpm --filter app-web test workflow`
- 浏览器手动验证：
  1. 创建新 lifecycle，单 activity，agent + executor_terminal，保存
  2. 加第二个 activity（function api_request），连接 artifact transition with binding
  3. 加第三个 activity（human approval），连接 human_decision_equals condition transition
  4. 切换某 activity executor 从 agent 到 human，确认 completion_policy 联动 + toast
  5. 编辑 iteration_policy 设 max_attempts=3 + alias=per_attempt + join_policy=any
  6. 校验 + 保存，再次加载确认所有字段往返一致
