# Lifecycle gate launch owner convergence

## Goal

在当前 PR 分支内，用一个 Trellis task 按 D4 -> D3 -> D2 串行收束三组剩余业务：

- D4：统一 launch command/source 模型，删除 AgentRun、RuntimeSession、FrameLaunch 之间的重复启动语言和映射环。
- D3：统一 `LifecycleGate` transition 语言，把 durable gate fact 与 mailbox/session delivery side effect 分离。
- D2：在 D4/D3 定型后拆薄 `LifecycleDispatchService` 内部 owner，保留 public facade。

本任务的用户价值是让 runtime launch、human gate、lifecycle dispatch 三条控制面主干拥有单一事实源，避免后续 workflow、companion、routine、local relay 继续围绕旧模型增生局部桥接。

## Confirmed Decisions

- D4 canonical launch command owner：`agentdash-application-ports`。
- D3 gate resolver scope：shared `LifecycleGateResolver`，覆盖 Companion、Workflow HumanGate，并为未来 Routine gate 预留同一 transition 语言。
- Task shape：单 Trellis task + 三个串行阶段，不拆 child task。
- Execution order：D4 -> D3 -> D2。D2 不得先于 D4/D3 推进。
- 当前项目处于预研单线阶段，本 PR 内继续完成三组剩余业务可接受；不做兼容层或旧 API fallback。

## Requirements

### R1. Canonical Launch Command

- 在 `agentdash-application-ports` 定义唯一 production launch command/source/modifier 模型。
- canonical launch command 必须放在独立 `launch/` namespace 下管理，不能继续塞进 `frame_launch_envelope.rs`，也不能在 ports 根目录平铺多个 launch 相关文件。
- `FrameLaunchEnvelopeRequest` 直接消费 canonical command。
- `backend_selection` 必须从 launch identity 中拆出，作为 launch planner input，而不是 FrameConstruction 或 command source identity 的一部分。
- AgentRun、RuntimeSession、FrameLaunch 不再各自定义 production `LaunchCommand` / `LaunchSource` / modifier 模型。
- 删除 AgentRun -> RuntimeSession -> FrameLaunch -> application command 的映射环。
- 所有启动来源保持原业务语义，但只构造 canonical command：HTTP / AgentRun workspace、Workflow / Routine、Companion、Hook resume、Local relay。
- RuntimeSession launch module 只能保留 launch planning / preparation / orchestration / result 这类运行期职责；不得保留一个转发式 command shell。

### R2. Shared Lifecycle Gate Resolver

- 新增 shared gate transition 层，统一 Companion 和 Workflow HumanGate 的 durable gate state transition。
- `LifecycleGate` payload 只保存 request/decision fact 和稳定 delivery refs，不保存 mailbox delivery status blob。
- Companion-specific parent/child/human runtime context 解析、mailbox delivery、session notification 必须从 gate transition 中分离。
- Workflow HumanGate 不再直接写 `gate.payload_json` 后调用 `gate.resolve(...)`，而是走 shared resolver。
- `CompanionGateControlService` 可暂时保留为 facade，但内部只编排 resolver、context resolver、delivery adapters 和 notification adapter。

### R3. Lifecycle Dispatch Owner Split

- 保留 `LifecycleDispatchService` / dispatch facade 对外入口。
- 内部拆分 owner services：
  - `RunOrchestrationStarter`：graph planning、run/orchestration 创建与 lifecycle start。
  - `AgentRuntimeMaterializer`：LifecycleAgent、RuntimeSession、AgentFrame、anchor、delivery binding materialization。
  - `SubjectAssociationWriter`：subject association 与 `SubjectExecutionRef`。
  - `LifecycleRelationWriter`：lineage 与 gate opening；gate opening 消费 D3 的 resolver/opening port。
  - `OrchestrationReducerBridge`：提交 `NodeStarted` 等 reducer event，并持久化 updated run。
- `dispatch_common` 收敛为 coordinator，不再直接拥有所有副作用策略。
- graph-backed dispatch 必须保持同一组 `orchestration_id + node_path + attempt` 贯穿 materialization、anchor、reducer 和 response refs。

## Out of Scope

- 不做 UI 功能扩展。
- 不重构无关业务模块。
- 不引入兼容 fallback 或旧模型并行路径。
- 除非实现确实新增或变更持久化字段，否则不做数据库 migration。
- 不把 task 拆成 child tasks；阶段边界写在 `implement.md` 中。

## Acceptance Criteria

- [ ] AC1：D4 后，production code 中只剩一套 canonical `LaunchCommand` / `LaunchSource`，且不存在 `FrameLaunchCommand`、`runtime_launch_command`、`to_frame_launch_command`、`launch_command_from_frame_launch` 映射环。
- [ ] AC2：D4 后，`backend_selection` 只作为 planner input 进入 launch planning，不作为 launch command identity 或 FrameConstruction 的隐式字段。
- [ ] AC2a：D4 后，launch intent 类型集中在 `agentdash-application-ports/src/launch/`；`frame_launch_envelope.rs` 只保留 envelope、surface、port 和 commit 相关类型；RuntimeSession 不保留 production command wrapper。
- [ ] AC3：D3 后，Companion gate 与 Workflow HumanGate 均通过 shared `LifecycleGateResolver` 执行 durable transition。
- [ ] AC4：D3 后，gate payload 不再存 mailbox delivery status blob；delivery 状态留在 mailbox/session delivery 结果或诊断投影中。
- [ ] AC5：D2 后，`LifecycleDispatchService` public facade 保持可用，内部 graph planning、runtime materialization、subject association、relation/gate write、reducer bridge 分属独立 owner。
- [ ] AC6：graph-backed dispatch 的 materialization refs、anchor refs、`NodeStarted` reducer、ready queue 清理都使用同一 `orchestration_id + node_path + attempt`。
- [ ] AC7：三个阶段分别通过 focused static checks 和 focused `cargo check`，失败阶段不得继续进入下一阶段。
- [ ] AC8：相关 `.trellis/spec/` 文档按最终 owner 契约更新，不记录只对当前任务有意义的临时实现过程。
