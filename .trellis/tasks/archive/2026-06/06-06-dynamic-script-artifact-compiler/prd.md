# Dynamic Script Artifact Compiler

## 目标

在 Orchestration domain contract、WorkflowGraph compiler 和 common orchestration runtime 已经成立后，引入动态脚本资产与脚本编译器前端：让模型生成或用户编写的 workflow script 经审批后编译为 `OrchestrationPlanSnapshot`，并复用现有 `LifecycleRun.orchestrations[]` common runtime。

本任务的核心不是新增第二套脚本运行时，而是新增一种 definition frontend。脚本和静态 `WorkflowGraph` 一样，只能作为 `OrchestrationPlanSnapshot` 的来源；runtime node state、state exchange、scheduler、terminal resolver、VFS surface、projection 和 trace anchor 继续走 Orchestration 正式路径。

## 背景

父任务 `.trellis/tasks/06-06-dynamic-workflow-lifecycle-research` 的原始目标是用 Claude Dynamic Workflows 资料压力测试 AgentDash Lifecycle / Orchestration 架构。当前已完成的地基包括：

- `LifecycleRun.orchestrations[]` 作为 0..N `OrchestrationInstance` 的 owning aggregate 字段。
- 静态 `WorkflowGraph -> OrchestrationPlanSnapshot` deterministic compiler。
- common orchestration runtime：entry materialization、semantic node executor launcher、state exchange、terminal callback、projection 和旧 Activity runtime 事实源清理。

因此本任务追踪剩余缺口：脚本化描述语法、脚本资产生命周期、脚本到 plan IR 的 compiler，以及运行前审批与保存为 workflow 的产品边界。

## 需求

- 定义脚本来源：
  - `RunScriptArtifact`：本次运行中由模型生成或用户临时编辑、经审批后启动的脚本。
  - `WorkflowScriptDefinition`：可复用项目/库级 workflow script 资产。
- 首版脚本形态采用 restricted Rhai workflow builder DSL，原因是 Hook 已经使用 Rhai，动态 workflow 又需要与 hook/gate/policy 约束协同；统一表达层可以复用沙箱、AST 缓存、语法校验、诊断和开发者心智。
- 将现有 Hook Rhai 引擎的通用能力抽出为公共脚本内核，至少覆盖 sandbox limits、AST cache、helper/module registration、validate/eval 和 JSON value bridge；Hook 与 Workflow Script 分别注册自己的 surface。
- 定义最小脚本语法或 AST，覆盖 Claude Workflow 核心行为族：
  - `phase`
  - `log`
  - `agent`
  - `parallel`
  - `pipeline`
  - `function`
  - `local_effect`
  - `human_gate`
  - state variable / artifact binding
  - args / limits / cache key / budget metadata
- 编译器输出 `OrchestrationPlanSnapshot`，并复用已有 `PlanNodeKind`、`ExecutorSpec`、`ActivationRule`、`StateExchangeRule`、`OrchestrationLimits`、`plan_digest`。
- 脚本编译器必须是 definition compiler：
  - 不读 repository。
  - 不创建 `LifecycleRun`。
  - 不启动 AgentRun / FunctionRun / LocalEffect。
  - 不执行 shell / 文件系统 / 网络副作用。
  - 不绕过 permission / capability surface。
- Rhai 脚本可以在编译期被解释执行，但执行结果只能是 workflow builder AST / plan builder document；不能直接调用 AgentRun、FunctionRun、LocalEffect 或 hook side effect。
- 定义 pathful diagnostics，覆盖语法错误、未知原语、非法变量引用、循环/并发上限缺失、executor spec 缺失、权限声明缺失、无界 fanout 或无法编译的动态形态。
- 设计运行前审批流：脚本源码、args、limits、plan preview、diagnostics 和 capability summary 必须在创建正式 `OrchestrationInstance` 前可见。
- 设计保存为 workflow 的边界：临时 run artifact 可保存为 `WorkflowScriptDefinition`，但保存动作不改变已运行实例的 plan snapshot 身份。
- 明确与现有静态 graph 的关系：graph compiler 与 script compiler 可以共享 IR helper，但不能共享一段“把 graph 模拟成脚本”的逻辑。
- 更新 research / spec 索引，让后续实现前能恢复 Claude Workflow 行为覆盖、当前 runtime 合同和脚本 frontend 目标。

## 非目标

- 不实现独立 JS/TS 沙箱 runtime。
- 不新增平行 scheduler。
- 不改变 `Lifecycle` / `Orchestration` 分层命名。
- 不把脚本运行态存进新的 graph instance 或 workflow instance 仓储。
- 不要求一比一复刻 Claude Code 的命令名、UI、目录结构或限制数值。

## 验收标准

- [x] `prd.md`、`design.md`、`implement.md` 明确脚本资产、Rhai builder DSL、公共脚本内核、compiler、审批流、保存流和 runtime 复用边界。
- [x] design 证明脚本 frontend 能覆盖 `research/claude-workflow-behavior-coverage.md` 的核心行为族，并说明暂不实现或以 diagnostics 阻塞的行为。
- [x] design 明确 `RunScriptArtifact` 与 `WorkflowScriptDefinition` 的身份、revision、digest、provenance 和权限边界。
- [x] design 给出 script AST -> `OrchestrationPlanSnapshot` 的映射表。
- [x] design 明确 Hook Rhai 与 Workflow Rhai 共享内核但隔离业务 surface 的模块边界。
- [x] implement 记录第一批 fixtures、diagnostics、contract DTO、API route、migration 判定、frontend service surface 和验证命令。
- [x] context manifests 指向父任务 research、common runtime 子任务、workflow specs 和 cross-layer contract specs。
- [x] 用户已确认以 Rhai builder DSL 作为当前实现路径，任务进入实现收口。

## 备注

- 父任务：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research`。
- 依赖已完成方向：`orchestration-domain-contract`、`workflow-graph-compiler`、`common-orchestration-runtime-static-graph`。
- 关键约束：脚本只新增 compiler frontend；所有运行态继续进入 `LifecycleRun.orchestrations[]`。
