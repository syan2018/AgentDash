# Lifecycle Designer 基于 Activity 模型重设计 — PRD

## 背景

Lifecycle 编辑器（`lifecycle-editor-shell.tsx` + `step-inspector.tsx` + `lifecycle-dag-canvas.tsx`）当前心智仍是"workflow contract 平铺 + 一个步骤"，与新 Activity 模型不对齐：

- Activity 顶层有 3 种 `executor.kind`（agent/function/human），function 还分 api_request/bash_exec，human 当前只接入 approval
- `completion_policy` 5 种（output_ports / executor_terminal / human_decision / hook_gate / open_ended）
- `iteration_policy.max_attempts/artifact_alias`、`join_policy` 4 种
- `transition.condition` 4 种（always / artifact_field_equals / human_decision_equals / agent_signal_equals），但 UI 只接入 2 种
- `transition.artifact_bindings` 是新模型核心能力，目前没有专属 UI

依赖 `05-21-lifecycle-step-fallback-cleanup` 先落地。

## 目标

把 Lifecycle Designer 重塑成 Activity 一等公民的编辑体验，覆盖新模型全部能力。

## 范围

### Activity 卡片化（DAG 节点）

- 节点徽章直接展示 `executor.kind` + `completion_policy.kind` 组合
- 鼠标悬停展示 attempts policy / join policy / port 数
- entry activity 视觉强调
- 错误/警告（来自 validation）可视化在节点角标

### Inspector 三段式

按 Activity 维度组织：

1. **Identity**：key、description、entry 切换、iteration_policy、join_policy
2. **Executor**：按 `kind` 切换不同表单
   - Agent: `workflow_key` + `session_policy`（spawn_child / continue_root / attach_existing）
   - Function (api_request): method / url_template / body_template (JSON editor)
   - Function (bash_exec): command / args / working_directory
   - Human (approval): form_schema_key / title
3. **Ports & Policy**：input_ports、output_ports、completion_policy（按 kind 切换：选 required_ports、选 decision_port、选 hook_key 等）

### 模式简化

取消 Form/DAG 的 sticky 双模式 + localStorage 粘性，统一 DAG。单 activity 时画布自动适配，不再有第二种 layout。

### Transition 编辑器

- 支持完整 4 种 condition，按 kind 切换不同字段
- 新增 artifact_bindings 编辑面板：from_activity.from_port → to_port，alias 选择
- 选中边时 inspector 切到 transition 视图（不是当前的 sidebar 列表）

### 校验与 UX

- 校验结果在节点角标 + Inspector 顶部 + 全局 ValidationPanel 三处呈现
- 保留 Ctrl+S、beforeunload、isDirty 状态

## 非目标

- 后端 schema 变更
- 运行时视图重设计（独立任务）
- form_schema_key 的可视化 schema 编辑（仍是字符串引用）

## 验收标准

1. 单/多 Activity 编辑流程在浏览器可走通
2. 3 种 executor kind 都能创建并保存
3. 所有 5 种 completion_policy 能选择并校验
4. 所有 4 种 transition condition 能编辑
5. artifact_bindings 能可视化编辑
6. typecheck/lint/test 通过；spec 更新 `frontend/workflow-activity-lifecycle.md`
