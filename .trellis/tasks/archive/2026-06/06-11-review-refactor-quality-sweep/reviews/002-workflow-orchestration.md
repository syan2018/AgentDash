# REVIEW-002: workflow-orchestration

## 范围

- `crates/agentdash-application/src/workflow/orchestration/**`
- 直接相关的 workflow dispatch / preflight / tools 边界

## 实现级可修复问题

### WF-IMPL-001: human gate decision port mismatch 诊断不可达

- 证据：`crates/agentdash-application/src/workflow/orchestration/script_compiler.rs:845` `compile_human_gate` 由 `decision_port` 直接构造 `outputs`，随后又检查 `decision_port` 是否存在于 `outputs`。
- 影响：`human_gate_decision_port_mismatch` 分支在非空时永远不可达，误导维护者以为 human gate 支持“声明 outputs 与 decision_port 分离”的模型。
- 建议：删除不可达 mismatch 诊断；如果需要校验，则让 builder document 显式携带 outputs，再由 compiler 校验 decision_port 属于 outputs。

### WF-IMPL-002: LocalEffect 是可编译但运行必失败的架空链路

- 证据：`crates/agentdash-application/src/workflow/orchestration/script_compiler.rs:777` `compile_local_effect` 会生成 `ExecutorSpec::LocalEffect`；`crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs:552` 遇到 `ExecutorSpec::LocalEffect` 恒定返回 `local_effect_capability_not_supported`。
- 影响：脚本层暴露了一条可编译、可 preflight 汇总、但运行期必失败的链路。
- 建议：二选一硬收敛：接入真实 LocalEffect executor；或在 parser/compiler 阶段把 `capability_effect` 作为 blocking diagnostic，避免生成可运行 plan。

### WF-IMPL-003: capability summary 有两套解释器

- 证据：`crates/agentdash-application/src/workflow/orchestration/script_compiler.rs:197` 内部有 `CapabilitySummaryBuilder`，并在 `378-406` 输出 `capability_summary`；`crates/agentdash-application/src/workflow/script/preflight.rs:267` 又调用 `extract_workflow_script_capability_summary`，`304-392` 维护另一套 typed extractor；API 最终返回 preflight 这套 summary。
- 影响：同一业务语义有两个解释器，后续新增 primitive 时容易出现 plan metadata 与 API capability summary 不一致。
- 建议：保留一个事实源。优先让 compiler 返回 typed `WorkflowScriptCapabilitySummary`，API 直接消费；plan metadata 只序列化这份 typed summary。

### WF-IMPL-004: runtime 从 plan metadata 反读 root args

- 证据：`crates/agentdash-application/src/workflow/orchestration/runtime.rs:83` 从 `plan_snapshot.metadata["script"]["args"]` 和 `root_input_bindings` 反读输入；`script_compiler.rs:1224` 把 args、bindings、summary 都塞进 metadata。
- 影响：runtime reducer 依赖 script compiler 的私有 JSON 形状；args 进入 plan metadata 后会影响 plan digest，削弱“plan snapshot 是不可变静态计划”的语义。
- 建议：把 root args materialization 从 plan metadata 移出，改成 activation 输入或 typed plan extension；metadata 只保留审计/展示信息。

### WF-IMPL-005: 未使用的 lenient 编译模式

- 证据：`crates/agentdash-application/src/workflow/orchestration/compiler.rs:17` `WorkflowGraphCompileMode::LenientDiagnostics` 只在 compiler 内部影响 artifact edge 诊断；`crates/agentdash-application/src/workflow/dispatch_service.rs:741` 固定 `Strict`，未见生产路径使用 lenient。
- 影响：预研期保留未接入的兼容式编译模式，增加诊断语义分叉。
- 建议：删除 `LenientDiagnostics` 和相关分支，统一 blocking 规则。

### WF-IMPL-006: `complete_lifecycle_node` 描述与运行事实不一致

- 证据：`crates/agentdash-application/src/workflow/tools/advance_node.rs:17` 与 `75-78` 声称 `complete_lifecycle_node` 是“唯一推进路径”；但 launcher 已通过 Function/HumanGate/Agent start 自动提交 runtime event，见 `executor_launcher.rs:362` 和 `orchestrator.rs:298`。
- 影响：命名/说明与运行事实不一致，会误导 agent prompt、hook 规则和维护者。
- 建议：改名或改描述为“Agent session 节点的主动 terminal 提交工具”，不要声明全局唯一推进。

## 架构 backlog 候选

### WF-ARCH-001: ready node 启动链路有两套入口

- 证据：`crates/agentdash-application/src/workflow/dispatch_service.rs:330` `dispatch_common` 对 graph-backed dispatch 自己创建 session/frame/anchor，并在 `409-423` 提交 `NodeStarted`；`OrchestrationExecutorLauncher::drain_ready_nodes` 在 `executor_launcher.rs:119-175` 也负责从 ready queue 启动 AgentCall/Function/HumanGate。
- 影响：同一“ready node -> executor side effect -> NodeStarted”链路有两套入口，subject dispatch 与 lifecycle start 的运行事实路径不同。
- 建议：统一为 `dispatch_common` 只创建/确保 run + orchestration，所有 ready node 启动都交给 launcher；或把 launcher 拆成唯一 scheduler port，dispatch 只调用它。

### WF-ARCH-002: `OrchestrationExecutorLauncher` 协调职责过宽

- 证据：`crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs:67` 持有 run、procedure、agent、frame、gate、anchor、runtime_session_creator、function_runner；同文件还创建 agent/frame/session/gate、执行 function/bash、写 reducer event。
- 影响：application 层协调器过宽，agent lifecycle、session delivery、human gate、function runner 四类职责集中在一个类，后续很难替换 executor 或做权限/预算边界。
- 建议：拆成 typed executor services：`AgentNodeLauncher`、`FunctionNodeRunner`、`HumanGateLauncher`；顶层 scheduler 只做 ready node selection 和 reducer write-back。

### WF-ARCH-003: ReadyNodeTarget 裸传坐标和快照

- 证据：`crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs:681` `ReadyNodeTarget` 长程复制 `run_id/orchestration_id/node_id/node_path/attempt/kind/plan_node/runtime_node/state_snapshot`；launcher 多个函数继续裸传这个 target。
- 影响：坐标、plan、runtime snapshot 混在一个临时 DTO，容易在 reload/update 后使用过期 runtime_node/state_snapshot，也掩盖真正的 runtime node coordinate。
- 建议：引入 typed `RuntimeNodeCoordinate` 和 `ReadyNodeLease/View`，side effect 只携带坐标，执行前按需重新加载 plan/runtime state。

### WF-ARCH-004: 生命周期状态事实源分散

- 证据：`crates/agentdash-application/src/workflow/orchestration/runtime.rs:924` reducer 内 `derive_orchestration_status`、`sync_lifecycle_run_status_from_orchestrations` 聚合状态；`run.rs:3` active run 选择、projection/view builder 对状态再次解释。
- 影响：Blocked/Paused/Ready/Running 的优先级容易在 scheduler、view、选择 active run 时分叉。
- 建议：把状态聚合提升为 domain/application 共享的 lifecycle status projector，所有 view/selection/scheduler 只消费同一 projector。
