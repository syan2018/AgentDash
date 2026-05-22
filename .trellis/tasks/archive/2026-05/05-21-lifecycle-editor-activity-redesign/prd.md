# Lifecycle Designer 基于 Activity 模型重设计 — PRD

## 背景

Lifecycle 编辑器（`lifecycle-editor-shell.tsx` + `step-inspector.tsx` + `lifecycle-dag-canvas.tsx`）当前心智仍是"workflow contract 平铺 + 一个步骤"，与新 Activity 模型不对齐：

- Activity 顶层有 3 种 `executor.kind`（agent/function/human），function 还分 api_request/bash_exec，human 当前只接入 approval
- `completion_policy` 5 种（output_ports / executor_terminal / human_decision / hook_gate / open_ended）
- `iteration_policy.max_attempts/artifact_alias`、`join_policy` 4 种
- `transition.condition` 4 种（always / artifact_field_equals / human_decision_equals / agent_signal_equals），但 UI 只接入 2 种
- `transition.artifact_bindings` 是新模型核心能力，目前没有专属 UI

依赖 `05-21-lifecycle-step-fallback-cleanup` 已落地。

## 目标

把 Lifecycle Designer 重塑成 Activity 一等公民的编辑体验，**一次性覆盖新模型全部能力**，没有"留待 Iter2"的能力门。

## 设计决策（brainstorm 落定）

- **范围 = 全量**：不切 MVP / Iter2，所有新模型能力本任务一次做齐。
- **Port 模型 = 折叠双层**：默认 UI 只显示 `ActivityDefinition.input_ports / output_ports` 一个统一列表；进阶可展开 "Workflow Contract 标准接口" 段独立编辑 `WorkflowContract.input_ports / output_ports`。`mergeContractIntoStep` 同步逻辑保留。多 activity 引用同一 workflow_key 时 contract.ports 是共享标准接口，不被单个 activity 编辑污染。
- **Inspector = 三段长滚动 + Contract 折叠**：取消 Overview/Detail tabs；Identity / Executor / Ports & Policy 三段栈式排布，每段顶部 sticky 子标题；Agent activity 的 Workflow Contract 子区（Injection / Capability / HookRules / Contract Ports）作为可折叠 collapsible details 放在 Ports & Policy 之后，默认折叠。选中 transition 时整 inspector 切换为 transition 编辑表单（替换关系，不是叠加）。
- **artifact_bindings = Canvas 创建 + Inspector 精修**：Canvas 上端口拖拽连线创建 binding（已有），选中 transition 后 inspector 列表式编辑 alias / 多 binding / from-to port，canvas 与 inspector 双向同步。transition.kind 从 artifact 改成 flow 时清空 bindings 并 toast 提示。
- **Executor × completion_policy 联动 = 智能保留**：切 executor.kind 触发 `ensurePolicyForExecutor(current, newKind)` 兼容矩阵：agent/function 不兼容 `human_decision`，function 额外不兼容 `hook_gate / open_ended`，human 仅兼容 `human_decision`。重置时 inspector 顶部 toast 提示原因。Entry activity 仍禁止 function executor（因为 entry 启动时无上游 inputs，function template 渲染缺上下文）。
- **单 Activity 画布 = 朴素无引导**：取消 Form 模式后，单 activity 直接在 DAG 画布上展示，不加任何 onboarding hint / 中央 CTA。Toolbar `+ 添加节点` 按钮即满足扩展入口。单 activity 是合法终态（freeform、简单 workflow），不催加节点。
- **Canvas 视觉 = 极简 + 分层**：节点常驻显示 executor icon + activity.key + completion_policy 4 字符徽章（OUT/EXE/HUM/HOOK/OPEN），entry 用 ring-2 强调，validation 错误用右上角红点；hover 200ms 弹出 tooltip 显示 description / executor 详情 / iteration_policy / join_policy / validation issues。边常驻显示 flow/artifact 线型 + 非 always 的 condition label（截断 30 字）+ max_traversals>1 时 ↻N icon；hover 弹出完整 condition + bindings 预览。不在节点常驻显示 iteration / join / workflow_key 等深度信息。

## 范围

### Activity 卡片化（DAG 节点）

- 节点徽章直接展示 `executor.kind` + `completion_policy.kind` 组合
- 鼠标悬停展示 iteration_policy / join_policy / port 数
- entry activity 视觉强调
- 错误/警告（来自 validation）可视化在节点角标

### Inspector 三段式

按 Activity 维度组织（取代当前 Overview/Detail tabs）：

1. **Identity**：key、description、entry 切换、`iteration_policy.max_attempts` & `artifact_alias`、`join_policy`（含 `n_of_m.n` 数字）
2. **Executor**：按 `kind` 切换不同表单
   - Agent: `workflow_key` + `session_policy`（spawn_child / continue_root / attach_existing）
   - Function `api_request`: method / url_template / body_template (JSON editor) — **新接入**
   - Function `bash_exec`: command / args / working_directory
   - Human `approval`: form_schema_key / title
3. **Ports & Policy**：input_ports、output_ports、`completion_policy`（按 kind 切换：output_ports → 选 required_ports；human_decision → 选 decision_port；hook_gate → 输入 hook_key 字符串；executor_terminal / open_ended → 无额外字段）

### Workflow contract 子区（Agent activity 限定）

Agent activity 的 inspector 中保留对 `WorkflowContract` 的编辑入口（Injection / Capability / HookRules / contract.ports），与 Activity 顶层的 ports / policy 分区呈现，让"workflow 资产 contract" 与 "Activity-on-lifecycle 配置"两层语义清晰。

### 模式简化

取消 Form/DAG 的 sticky 双模式 + localStorage 粘性，**统一 DAG 单一 layout**。单 activity 时画布只画一个节点 + 隐藏 minimap/控制条 chrome；右侧仍是 inspector，无第二种 layout。

### Transition 编辑器（选中边时切换）

- 支持完整 4 种 condition，按 kind 切换不同字段：
  - `always`
  - `artifact_field_equals`：activity / port / path（JSON path）/ value
  - `human_decision_equals`：activity / decision_port / value
  - `agent_signal_equals`：activity / signal_key / value
- `transition.kind`（flow / artifact）显式可改
- `transition.max_traversals` 数字输入
- **artifact_bindings 可视化编辑面板**：列表式 from_activity.from_port → to_port + alias（latest / per_attempt / latest_and_history），可增删、可下拉选择来源 activity 与 port
- 选中边时 inspector 切到 transition 视图（不是当前的 bottom panel）

### 校验与 UX

- 校验结果在节点角标 + Inspector 顶部 + 全局 ValidationPanel 三处呈现
- 保留 Ctrl+S、beforeunload、isDirty 状态

## 非目标

- 后端 schema 变更
- 运行时视图重设计（独立任务 `05-21-lifecycle-runtime-view-activity-redesign`）
- form_schema_key 的可视化 schema 编辑（仍是字符串引用）
- hook_gate 的 hook_key preset 选择器与跨模块联动（**MVP 用纯字符串输入**；hook preset 后续接入需后端先暴露 lifecycle-scope hook 注册表）

## 验收标准

1. 单/多 Activity 编辑流程在浏览器可走通
2. 3 种 executor kind 都能创建并保存（含 Function api_request、bash_exec、Human approval、Agent 三种 session policy）
3. 所有 5 种 completion_policy 能独立选择、保存、校验
4. 所有 4 种 transition condition 能编辑、保存、校验
5. artifact_bindings 能在 transition 详情面板中可视化增删改
6. iteration_policy（max_attempts、artifact_alias）与 join_policy（含 n_of_m.n）能编辑
7. transition.kind 与 max_traversals 能编辑
8. DAG 节点徽章能看到 executor + completion_policy 组合
9. typecheck/lint/test 通过；spec 更新 `frontend/workflow-activity-lifecycle.md`

## 开放问题（brainstorm 待解决）

（无剩余 brainstorm 开放问题；进入 design / implement 文档撰写。）
