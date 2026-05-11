# Workflow/Lifecycle 前端编辑统一为单 editor 自适应布局

## Goal

把前端"并列双实体"式的 Workflow / Lifecycle 编辑体验，收束成"一个 editor，按 step 规模自适应布局"。
让用户对**一个 workflow 行为约束**和**一条 DAG 编排**的心智模型回归统一 —— 两者是同物种的不同规模，不是两个资产。

## Background（前两轮对话共识）

### 问题诊断

- 领域上：`LifecycleStepDefinition.workflow_key` 引用 `WorkflowDefinition`，是**嵌套**关系
- Spec 上：[lifecycle-edge.md:91](../../../.trellis/spec/backend/workflow/lifecycle-edge.md#L91) 明确允许"单 step lifecycle"（无需 edges），即"装载一个 workflow" = "单 step lifecycle"
- 运行时上：`start_workflow_run` 强制要求 `lifecycle_id`/`lifecycle_key`（[run.rs:361-368](../../../crates/agentdash-application/src/workflow/run.rs#L361-L368)），workflow 不能脱离 lifecycle 运行
- 前端上：却把两者拍平成两个顶级资产——两个 tab、两套 CRUD、两个 editor、两套 EditorState

### 症状（用户观察）

1. 多数 workflow 跟随 lifecycle 使用，独立编辑场景少
2. 单 workflow 装载能处理很多场景，把用户塞进 DAG 编辑器去编一个孤立 workflow 很怪
3. 当前 DAG 编辑器里对 workflow 的编辑嵌套太深：`全屏 DAG → DagSidePanel → "编辑 workflow" 按钮 → DetailPanel 抽屉 1481 行 WorkflowEditor`，三层堆叠

### 当前代码规模

| 文件 | 行数 | 角色 |
|---|---|---|
| [workflow-editor.tsx](../../../frontend/src/features/workflow/workflow-editor.tsx) | 1481 | Workflow contract 表单编辑器 |
| [lifecycle-dag-editor.tsx](../../../frontend/src/features/workflow/lifecycle-dag-editor.tsx) | 978 | Lifecycle DAG 编辑器 |
| [workflow-tab-view.tsx](../../../frontend/src/features/workflow/workflow-tab-view.tsx) | 456 | Workflow / Lifecycle 并列 tab |
| [workflowStore.ts](../../../frontend/src/stores/workflowStore.ts) | 756 | 双 EditorState 镜像 |
| [WorkflowCategoryPanel.tsx](../../../frontend/src/features/assets-panel/categories/WorkflowCategoryPanel.tsx) | 542 | Assets 页 Workflow 类目（也是并列 tab） |
| [dag-side-panel.tsx](../../../frontend/src/features/workflow/ui/dag-side-panel.tsx) | 512 | DAG 节点侧栏 |

## 方向（前两轮已定）

**方向 A'：一个 Editor，按 step 规模自适应三种 layout**

| 形态 | 触发条件 | UI 呈现 |
|---|---|---|
| **Form 模式** | 1 个 step + agent_node | 纯 contract 表单（Injection/Hooks/Capability/Ports），无画布、无侧栏 |
| **DAG 模式** | ≥ 2 个 step | 左：DAG 画布；右：选中 step 的 contract 面板（inline，**不再开抽屉**） |
| **过渡态** | Form 模式加第二个 step | 自动展开为 DAG，原 step 变第一个节点 |

**核心改造**：
1. 拆 `workflow-editor.tsx` 为可独立挂载的 panel 组件（`<InjectionPanel>` / `<HookRulesPanel>` / `<CapabilityPanel>` / `<PortsPanel>` / `<BasicInfoPanel>`）
2. 杀掉 `DetailPanel`-over-`DetailPanel` 的抽屉嵌套；DAG 侧栏 inline 切 panel
3. 前端 store 合并 `lcEditor` + `wfEditor` 为单 editor；后端 schema **保持不变**（双实体）
4. 顶级用户心智命名统一为"Workflow"；Lifecycle 退化为"多 step 模式下的内部 DAG 编排"
5. Workflow 复用显式化但不喧宾夺主（单 step 模式小按钮弹 popover，不开顶级资产页）

## What I already know

- 后端已允许单 step lifecycle（不动 schema 的大前提存在）
- Step 已分 `agent_node` / `phase_node`（[workflow.ts:284](../../../frontend/src/types/workflow.ts#L284)）
- Step 级 `capability_config` 与 workflow contract 级 `capability_config` 并存（应用顺序：先 workflow，后 step）
- Builtin Bundle 模板（[trellis_dag_task.json](../../../crates/agentdash-application/src/workflow/builtins/trellis_dag_task.json)）每个 lifecycle 自带 N 个 workflow，bootstrap 时一次性落库
- 路由：`/lifecycle-editor/:id`（全屏）+ workflow editor 的 `DetailPanel` 抽屉；`/workflow-editor/:id` 也被 App.tsx 预留但只在 Assets 面板引用
- Workflow 复用目前仅通过 step 的 `workflow_key` 字符串引用实现（跨 step / 跨 lifecycle 引用同一 workflow 技术可行，实际 builtin 里未见用例）

## Assumptions (待验证)

- A1. 独立 Workflow 复用场景存在但罕见，UI 上降级为 popover 入口可接受
- A2. 用户对"单 step + agent_node 才能进 Form 模式"没异议（phase_node 没有 contract，Form 模式不适用）
- A3. Form ↔ DAG 的模式切换由 `steps.length` 自动决定，不需要显式"切换模式"按钮
- A4. 后端双实体 schema 保留，本任务纯前端改造

## Open Questions

（全部已解决，见 Decision Log）

## Technical Approach

### 组件与状态机

```
LifecycleEditorShell (新)
├─ props: lifecycleId | "new"
├─ state: mode = steps.length === 1 && edges.length === 0 && !sticky_dag
│                   ? "form" : "dag"
├─ Form 模式渲染：
│   └─ <LifecycleHeader /> + <StepContractPanels />（panel 平铺）
└─ DAG 模式渲染：
    └─ <LifecycleHeader /> + <DagCanvas /> + <StepContractPanels inline />（右侧栏，不开 DetailPanel）

StepContractPanels（受控）
├─ <BasicInfoPanel />
├─ <InjectionPanel />
├─ <HookRulesPanel />
├─ <CapabilityPanel />
└─ <PortsPanel />
  （props 受控：step 数据 + onChange；不直接依赖 store）
```

### Store 合并

- 废：`wfEditor` + `lcEditor` 两组镜像 state
- 立：`lifecycleEditor` 单 state，包含 lifecycle draft + 当前选中 step key + 该 step workflow draft 的 inline 视图
- 保存：单 editor 触发一次保存 → 内部分别调 `updateLifecycle` + `updateWorkflow`（顺序：先 workflow，后 lifecycle）；失败回滚策略由后端 API 管（本任务不改）

### Clone 机制

- 新建 step 时自动生成 `workflow_key = <lifecycle_key>.<step_key>`
- "从已有 Workflow 克隆"入口：Form/DAG 模式下点"从模板创建"按钮 → popover 选现有 workflow → clone 其 contract 作为当前 step 的初始值
- 不出现"引用同一 workflow_key 的多 step"语义（即使技术上可行）

### phase_node 与 Form 模式

- **更正（2026-05-11）**：之前 Decision Log 里写的"phase_node 没有 workflow contract"是错的。领域层 `LifecycleStepDefinition.workflow_key` 从未限定 node_type，phase_node 与 agent_node 一样可以绑 workflow contract。老 `dag-side-panel.tsx` 的 `isAgentNode` 是 UI 偏见，不是领域约束。
- 唯一硬约束：entry step 必须是 agent_node（[lifecycle-edge.md §4](.trellis/spec/backend/workflow/lifecycle-edge.md)）
- Form 模式的单 step 默认 agent_node（因为它就是 entry），但若用户在 Form 模式想切 phase_node 应被允许（仅当不是 entry 时）—— 实际 Form 模式只有一个 step 即 entry，所以单 step Form 模式下永远 agent_node 是结果不是约束
- DAG 模式下 phase_node 同样可编辑完整 workflow contract

### 侧栏布局（Overview / Detail 双 tab）

DAG 模式下右侧栏（保持窄宽度，约 w-96）顶部有两个 tab 切换：

- **Overview**（节点外部接口视图）：key、name、description、node_type、input_ports、output_ports —— 对齐 DAG 画布上一眼能看到的"标注信息"，是编排者视角
- **Detail**（workflow contract 编辑）：完整 5 panel（Basic / Injection / Hooks / Capability / Ports）—— 是 step 行为约束的细节视角

panel 组件需要修复响应式样式以适配窄宽度（避免 grid-cols-2 导致挤压、过宽 input、过长 label 等）。Form 模式宽容器仍用现有布局。

## Decision Log

- **2026-05-11** MVP 范围：分三步落地（PR1 panel 解耦 → PR2 store 合并 + 新 editor → PR3 回收老入口）。
  - Why：降低合并冲突风险；PR1 不改行为，PR2 切换入口前可充分 manual 验证；每步独立 ship。
- **2026-05-11** 命名 facade 深度：仅最外层导航 / 标签 / 头部文案改为 Workflow；前端类型、文件名、store slot 全部保留 Lifecycle 命名。
  - Why：Lifecycle 是领域层"运行时阶段编排"的通用范畴，不止服务于 workflow 编辑视角；前端代码保留 Lifecycle 命名准确反映领域语义。Workflow 是给"多数单 step 用户"的对外 facade。
  - How to apply：PR1/PR2/PR3 不做 rename；新增组件按 Lifecycle 命名（如 `lifecycle-editor-shell.tsx`）；只有 user-facing 字符串、tab 标签、navigation entry 用 Workflow。
- **2026-05-11** 路由策略：合并为单一 `/workflow/:id`，老路由 `/lifecycle-editor/:id` + `/workflow-editor/:id` 直接删除（项目未上线，不做 301 redirect）。
  - How to apply：PR2 上线新 `/workflow/:id`，PR3 删除旧路由和对应 App.tsx / workspace-layout 的 useMatch / remember path 逻辑。
- **2026-05-11** Workflow 复用语义：Clone（每个 step 各自有独立的 WorkflowDefinition 行）。
  - Why：与 memory `workflow_design_principle.md`"Workflow 是 agent 单步行为约束，跟随 step"一致；避免跨 step 共享导致的级联调试问题；为未来 schema 合并预留空间。
  - How to apply：前端不再暴露"独立 Workflow 资产"列表 / 路由；step 创建时自动生成 `workflow_key`（如 `<lifecycle_key>.<step_key>`）；现有 builtin bundle 的多 workflow 形态保持兼容（运行时 schema 不变）。
  - Follow-up：单独任务"WorkflowDefinition 与 LifecycleStep contract 合并（schema 收敛）"，不阻塞当前任务。
- **2026-05-11** Form ↔ DAG 过渡：保持 DAG 画布（单节点），不自动降级。
  - Why：避免误触时的视图抖动；用户进入 DAG 后心智一致。
  - How to apply：判定 DAG 模式的条件不是 `steps.length > 1`，而是 `steps.length > 1 || edges.length > 0 || entry_step 之外有历史 step`，或者显式的 "已经进入 DAG" sticky 标记。首次新建始终 Form。
  - 隐含：Form 模式仅适用于"首次新建 + 从未加过第二个 step"的对象；所有已保存过 ≥2 step 的 lifecycle 永远 DAG。

## Requirements

- R1. 前端顶级导航 / 资产面板合并为单一"Workflow"入口（文案层改，类型层保留 Lifecycle）
- R2. 编辑器按"steps 数 + edges 数 + sticky_dag 标记"自动选 Form / DAG 视图
- R3. 新建对象默认 Form 模式；一旦进入 DAG 不再自动降级
- R4. DAG 模式下 step contract 编辑在右侧栏 inline，废除 DetailPanel 嵌套
- R5. `workflow-editor.tsx` 拆为受控 panel 组件（`<BasicInfoPanel>` / `<InjectionPanel>` / `<HookRulesPanel>` / `<CapabilityPanel>` / `<PortsPanel>`），Form / DAG 两种模式复用同一套 panel
- R6. Store 层 `lcEditor` + `wfEditor` 合并为 `lifecycleEditor` 单 state
- R7. 路由合并为 `/workflow/:id`，旧路由直接删除
- R8. Workflow 复用走 Clone 语义（不暴露跨 step 共享）
- R9. 后端 schema / API / builtin bundle JSON 结构不变

## PR 拆分

- **PR1 — panel 解耦（不改行为）**
  - 从 `workflow-editor.tsx` 抽出 5 个受控 panel 组件
  - 原 `workflow-editor.tsx` 变容器，组合这些 panel（行为等价）
  - 单元测试：每个 panel 组件独立渲染 + onChange 覆盖
  - 验收：现有路由 / 入口 / UI 行为 100% 不变，`workflowStore` 不动

- **PR2 — 新 editor 上线（行为切换）**
  - 新增 `LifecycleEditorShell` + Form / DAG 自适应逻辑
  - 新增 `/workflow/:id` 路由
  - Store 层引入 `lifecycleEditor` 合并 state（保留老 `lcEditor`/`wfEditor` 以兼容老入口）
  - 主菜单 Workflow tab + Assets 面板 Workflow 类目切换到新 editor
  - DAG 模式下侧栏 inline 化，废除 DetailPanel 嵌套
  - Clone 机制上线

- **PR3 — 清理**
  - 删老路由 `/workflow-editor/:id` / `/lifecycle-editor/:id`
  - 删老组件 `workflow-tab-view.tsx` 里的 Workflow tab（保留 Lifecycle 列表的必要部分并入新 UI）、`lifecycle-dag-editor.tsx` 旧入口
  - 删老 store state `wfEditor` / `lcEditor`
  - 删 App.tsx / workspace-layout 的 useMatch / remember path 对旧路由的引用
  - 清理 `WorkflowCategoryPanel.tsx` 里 Workflow tab 分支
  - 更新文档 / memory

## Acceptance Criteria

- [ ] 新建 Workflow 默认 Form 模式，UI 中不显示 DAG 画布
- [ ] Form 模式下加第二个 step → 自动进入 DAG，原 step 成首节点
- [ ] DAG 模式下从 2 step 删回 1 step：画布仍保留（不自动降级）
- [ ] DAG 模式下选中节点，Panel 在右侧栏 inline 渲染，DOM 中不出现嵌套 DetailPanel
- [ ] 主菜单 Workflow 入口 + Assets 面板 Workflow 类目使用同一新 editor
- [ ] 旧路由 `/workflow-editor/:id` / `/lifecycle-editor/:id` 在 PR3 后 404
- [ ] builtin Bundle 注册按钮仍可用（功能 + 落库路径不变）
- [ ] 单 editor 覆盖原 Workflow / Lifecycle 全部 CRUD 能力（Injection / Hooks / Capability / Ports / Step 增删 / Edge 连接 / 验证 / 保存）
- [ ] 净代码行数减少 ≥ 500（frontend/src 目录）
- [ ] 所有现有单元测试通过 + 新增 `lifecycleEditorShell` 状态机测试

## Definition of Done

- 单元测试：Form ↔ DAG 切换 / panel 组件独立渲染 / 合并后的 store 状态机
- 现有 `workflowStore.test.ts` / `lifecycle-port-sync.test.ts` / `capability-directive-ops.test.ts` 全部通过
- 类型检查 + lint 清洁
- 手动走通：新建 Form → 升级 DAG → 加 artifact edge → 保存 → 重新打开 → 编辑 → 删步回 Form 的完整链路
- 前端 bundle size 变化有明确交代（目标：净减少行数 ≥ 500）

## Out of Scope

- 后端 schema 合并（方向 B）
- 后端 API 改动
- 领域层 `LifecycleDefinition` / `WorkflowDefinition` 合并
- Workflow Runtime（`WorkflowRun`、session view、step 激活等）的 UI
- 独立 Workflow 资产复用库的高级搜索/过滤
- 内置 trellis builtin workflow 的清理（另任务处理）

## Technical Notes

- 后端双实体保留，前端做 facade 组合
- 参考：memory `workflow_design_principle.md` —— Workflow 是 agent 单步行为约束，LifecycleStep 是其封装
- 参考：memory `session_context_builder_progress.md` —— 最近已有大量 frontend ContextFrame 工作在并行
- 风险：`lcEditor` / `wfEditor` 合并会波及 workflow store 所有调用方（DAG editor / tab view / assets panel / session view / binding editor 等），需逐点审
