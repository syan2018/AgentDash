# Lifecycle Designer Redesign — Implementation Plan

## 总体策略

按"自下而上"展开：先打基础（store actions + selection 模型 + executor 联动 helper），再做 inspector sections，最后切 shell + canvas + 删旧。每个阶段 typecheck 与目标 vitest 用例必须绿，再进下一阶段。

PR 单合并；不开 feature flag。

## Stage 1 — Store 层与 helper（无 UI 改动）

- [ ] 在 `packages/app-web/src/stores/workflowStore.ts` 把 `selectedActivityKey: string | null` 替换为 `selection: ActivitySelection | TransitionSelection | null`
- [ ] 新增 helper `transitionId(t: ActivityTransition, idx: number): string` 形如 `${t.from}-->${t.to}#${idx}`，导出供 canvas + inspector 共用
- [ ] 新增 store actions：
  - `selectLifecycleActivity(activityKey)` `selectLifecycleTransition(transitionId)`
  - `updateLifecycleEditorTransition(transitionId, patch)` / `setTransitionKind` / `addArtifactBinding` / `updateArtifactBinding` / `removeArtifactBinding`
  - `setActivityCompletionPolicy(activityKey, policy)` / `setActivityIterationPolicy(activityKey, patch)` / `setActivityJoinPolicy(activityKey, policy)`
  - `setActivityExecutor(activityKey, executor)` 内部调用下面的 helper
- [ ] 新增 `ensurePolicyForExecutor(current, newKind): { policy, reset }` helper，导出用于 store 与 ExecutorSection
- [ ] 跟进 `addLifecycleEditorActivity` / `removeLifecycleEditorActivity` / `updateLifecycleEditorActivity` 内对 selection 的引用（rename 时保持 selection 指向新 key）
- [ ] 单测：`workflowStore.test.ts` 扩展
  - executor 切换后 completion_policy 联动（agent→human 强制 human_decision、human→agent 落 executor_terminal、function 不允许 hook_gate 等矩阵全覆盖）
  - artifact_bindings 增删改
  - transition.kind=artifact→flow 清空 bindings
  - rename activity 时 transition.from/to + binding.from_activity 同步（已有，确认未回归）
- [ ] Gate：`pnpm --filter app-web typecheck` + `pnpm --filter app-web test workflow`

## Stage 2 — Inspector 子组件

每个 section 单独文件，无相互依赖（除了 sections 都依赖 store）。

- [ ] `ui/sections/IdentitySection.tsx`：
  - props: `activity`, `isEntry`, callbacks
  - 字段：key / description / set entry / iteration_policy（max_attempts 数字 + 无限复选 + alias select）/ join_policy（select + n_of_m.n 条件输入）
- [ ] `ui/sections/ExecutorSection.tsx`：
  - props: `activity`, `workflowDraft`, `isEntry`, callback
  - executor.kind select（function entry 时 disabled）
  - per-kind 表单：
    - `ui/sections/executor/AgentExecutorForm.tsx`（workflow_key + session_policy）
    - `ui/sections/executor/FunctionExecutorForm.tsx`（type select + api_request 子表单 + bash_exec 子表单）
    - `ui/sections/executor/HumanExecutorForm.tsx`（title + form_schema_key）
  - 切换时调 store 的 `setActivityExecutor`，捕获 `{reset}` 在 inspector 顶部 toast 提示
- [ ] `ui/sections/PortsAndPolicySection.tsx`：
  - input_ports / output_ports（沿用 `OutputPortItem` `InputPortItem`）
  - completion_policy 编辑器：
    - `ui/sections/policy/CompletionPolicyEditor.tsx` 内 switch 5 种 kind 渲染对应字段
- [ ] `ui/sections/WorkflowContractSection.tsx`：
  - `<details>` 折叠
  - children：InjectionPanel / CapabilityPanel / HookRulesPanel / PortsPanel（contract）
  - 通过 `mergeContractIntoStep` 同步 contract.ports → activity.ports（沿用现 step-inspector.tsx 的 handler 逻辑，迁过来）
- [ ] `ui/activity-inspector.tsx`：
  - sticky header（key + entry button + close）
  - 串联上面 4 个 section
  - 顶部 reset toast 槽
- [ ] `ui/conditions/*`：
  - `AlwaysCondition.tsx`（无字段，仅 select 切换其他 kind 时启用）
  - `ArtifactFieldEqualsCondition.tsx`
  - `HumanDecisionEqualsCondition.tsx`
  - `AgentSignalEqualsCondition.tsx`
  - 一个 `ConditionEditor.tsx` switcher
- [ ] `ui/ArtifactBindingsEditor.tsx`：
  - 列表式，row 含 from_activity select / from_port select / to_port select / alias select / 删除
  - "+ 添加 binding" 按钮
- [ ] `ui/transition-inspector.tsx`：
  - sticky header
  - kind switch（flow/artifact，artifact→flow 时 confirm + 清空 bindings）
  - ConditionEditor
  - max_traversals 数字 + 无限复选
  - ArtifactBindingsEditor（仅 kind=artifact 显示）
- [ ] 组件单测：每个 section / condition / binding editor 至少一条 happy path 渲染测试
- [ ] Gate：`pnpm typecheck && pnpm test workflow`

## Stage 3 — DAG Canvas 重塑

- [ ] `ui/dag-node.tsx` 重写：
  - props: `key`, `description`, `executorKind`, `executorBadge`（icon），`completionPolicyKind`，`isEntry`，`validationCount`，`inputPorts`，`outputPorts`
  - 视觉：徽章 + ring + 红点 + ports handles
  - 用 native `title` 简版 tooltip 暂时承载 hover 详情；如时间允许换 radix-ui Tooltip 并把 description / executor 详情 / iteration / join / validation 写进 tooltip body
- [ ] `ui/lifecycle-dag-canvas.tsx` 调整：
  - `stepsToNodes` 增加 completionPolicyKind / validationCount 等字段
  - edge label 算法：flow 实线 + 非 always condition label；artifact 虚线 + binding 摘要 + max_traversals>1 时 ↻N
  - edge 颜色按 condition kind：always=默认/灰；human_decision_equals=蓝；artifact_field_equals/agent_signal_equals=灰
  - `handleNodeClick` → store.selectLifecycleActivity
  - 新增 `handleEdgeClick` → store.selectLifecycleTransition；用 transitionId helper 生成 id
  - `handlePaneClick` → 清空 selection
- [ ] `lifecycle-dag-canvas.tsx` 移除 sticky_dag 桶相关逻辑（已迁到 shell）
- [ ] Gate：`pnpm typecheck && pnpm test workflow`

## Stage 4 — Shell 重塑

- [ ] `lifecycle-editor-shell.tsx` 改写：
  - 删除 Form 模式分支与 `mode` useMemo / `stickyDag` state / sticky 读写函数
  - 删除 `migrateStickyDag`（旧 sticky 桶残留可保留 LocalStorage key 不读写）
  - 整体 layout：固定 DAG canvas 左 + sidebar 右
  - sidebar 根据 `selection` 路由：
    - `selection.kind === "activity"` → ActivityInspector
    - `selection.kind === "transition"` → TransitionInspector
    - 否则 → LifecycleHeader（顶层信息）
  - TopBar / ValidationOverlay 保留
- [ ] 删除 `lifecycle-editor-shell.test.tsx` 中的 mode judgement 用例（不再适用）；扩展 store integration 用例覆盖 selection 路由
- [ ] 删除 `step-inspector.tsx` + `step-inspector.test.tsx`
- [ ] 新建 `activity-inspector.test.tsx` + `transition-inspector.test.tsx`（renderToStaticMarkup 形式）
- [ ] Gate：`pnpm typecheck && pnpm lint && pnpm test`

## Stage 5 — Spec & 浏览器手动验证

- [ ] 更新 `.trellis/spec/frontend/workflow-activity-lifecycle.md` 增加 Scenario "Lifecycle Designer 信息架构"，描述：
  - DAG-only layout
  - selection 模型驱动 sidebar
  - Inspector 三段 + Contract 折叠
  - Transition 编辑独立形态
  - executor × completion_policy 联动矩阵
- [ ] 浏览器走通 design.md 中"验证门"列出的 6 个手动用例
- [ ] 不自动 commit；等用户视觉验收

## 风险点 & rollback

| 风险点 | 表现 | rollback 策略 |
|---|---|---|
| Inspector 三段栈高度溢出 | 长 lifecycle 下垂直滚动疲劳 | 保留 sticky section header；如严重溢出降级到 accordion（成本：重写 toggle state） |
| Tooltip 简版 native title 不可定制 | hover 信息不够丰富 | 提级到 radix-ui Tooltip，已是受控组件，替换成本低 |
| `setActivityExecutor` 联动 reset 与用户预期不符 | 用户切回原 executor 后 policy 不复位 | 暂存 "lastSeenPolicy"；下个 PR 再做（本任务不上） |
| transitionId 派生不稳定（同 from-to 多边时 index 因 reorder 变化） | 选中态偶尔失效 | 选中后局部缓存 transition snapshot，匹配失败再 fallback 到 (from,to) 第一条 |

## Out of scope（明确）

- hook_gate.hook_key 的 preset 选择器（保持纯字符串输入）
- form_schema_key 的可视化 schema 编辑（保持纯字符串引用）
- workflow contract 跨 activity 复用的"提升为标准接口"专用 UI（手动编辑 contract.ports 已足够）
- 后端 schema / migration
- 运行时视图重设计（独立任务）
