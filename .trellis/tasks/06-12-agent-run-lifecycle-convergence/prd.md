# 收束 AgentRun 启动生命周期与前端交互契约

## Goal

把 AgentRun 启动、RuntimeSession delivery、AgentFrame runtime surface、workflow lifecycle mount 与前端 AgentRun Workspace 交互状态收束成一套可解释、可测试、可逐步实现的统一契约。

用户价值是让 ProjectAgent draft、显式 lifecycle、Workflow AgentCall、Routine、Companion、pending queue 与后续消息共享同一条启动生命周期模型，前端只消费稳定的 AgentRun workspace 状态，而不是跟随后端多处隐式推断漂移。

## Confirmed Facts

- `ProjectAgentRunStartService` 当前负责 ProjectAgent draft materialization：先通过 `LifecycleDispatchService` 创建 run / agent / launch frame / runtime session，再绑定 `LifecycleAgent.project_agent_id`，随后用 `AgentRunMessageService` 投递首条消息。
- `LifecycleDispatchService` 同时承载 graphless dispatch 与 graph-backed dispatch。graph-backed dispatch 会创建 `RuntimeSessionExecutionAnchor::new_orchestration_dispatch` 并把 entry runtime node 标记为 started；graphless dispatch 创建普通 dispatch anchor。
- `AgentFrameBuilder::new_launch_anchor` 表达 dispatch launch evidence revision；runtime surface 由 frame construction / composer 生成后续 revision。
- `FrameConstructionService` 当前以 RuntimeSession anchor 反查 run / agent / frame，然后通过 `classify` 选择 ProjectAgent、lifecycle node、companion 或 existing frame surface composer。
- 最近热修已证明仅靠 anchor 形状判断 composer 会误导 ProjectAgent + explicit lifecycle 入口；ProjectAgent identity 需要先决定 owner surface，同时 active workflow projection 继续提供 lifecycle mount。
- `AgentRunWorkspaceView` 已成为前端 workspace 的 public contract，包含 shell、delivery runtime ref、frame runtime、control plane、actions、pending queue 和 pending messages。
- `AgentRunWorkspacePage` 已使用 `/agent-runs/new` 和 `/agent-runs/:runId/:agentId`，draft start 成功后跳转正式 AgentRun route，后续消息、steer、enqueue、cancel 均走 AgentRun command endpoints。
- `useAgentRunWorkspaceState` 当前先 fetch workspace，再额外解析 `session_runtime` VFS surface；前端可交互性由 workspace projection status、control plane 与 actions 共同驱动。
- 既有 AgentRun workspace/API 任务已完成 route、generated contract、command receipt 与前端模型水合，但启动生命周期仍缺少统一 launch contract 作为后端/前端共同事实。

## Requirements

- 定义统一 `AgentLaunchPlan` 或等价 application contract，明确每次启动的 owner、delivery runtime、frame evidence、surface composer、active workflow、subject、command receipt 与 cleanup 策略。
- 将启动入口按 owner/source 明确分类：ProjectAgent owner、existing AgentRun continuation、workflow lifecycle node / AgentCall、Routine executor、Companion child、Hook auto-resume、pending queue drain、local relay/runtime action。
- 将 frame construction 的 composer 决策从“形状推断”收束为“launch plan / owner kind / active workflow”驱动；`RuntimeSessionExecutionAnchor` 继续作为 trace/delivery 反查索引。
- ProjectAgent explicit lifecycle 路径必须同时具备 ProjectAgent owner surface 与 lifecycle mount。
- Workflow AgentCall / lifecycle node 路径必须保留 node-scoped lifecycle surface，并与 ProjectAgent owner path 有可测试边界。
- AgentFrame revision 角色要在代码和测试中可识别：launch evidence revision、runtime surface revision、continuation surface reuse。
- AgentRun Workspace projection 需要直接表达 launch/control readiness：delivery missing、surface pending/frame missing、ready、running、cancelling、terminal、failed cleanup 等状态要有稳定语义。
- 前端 draft start、send next、enqueue、steer、cancel、pending resume 的 UI 状态必须来自后端 AgentRun workspace contract，而不是独立重建后端启动判断。
- 首轮消息失败、surface composition 失败、connector launch 失败、receipt duplicate/retry 要有一致 cleanup 和恢复语义。
- 测试矩阵必须覆盖所有一等入口，尤其是 ProjectAgent graphless、ProjectAgent explicit lifecycle、workflow AgentCall、pending queue drain 和 frontend draft-to-workspace transition。
- 规划完成后，再由后续实现任务分阶段修改代码；本任务本身不启动大规模实现。

## Acceptance Criteria

- [ ] `design.md` 包含当前状态 Mermaid 图，覆盖 ProjectAgent draft -> dispatch -> frame construction -> workspace projection -> frontend interaction 的现状链路。
- [ ] `design.md` 包含目标状态 Mermaid 图，明确统一 launch plan、composer selection、AgentRun workspace projection 与前端 command state 的边界。
- [ ] `design.md` 明确后端 launch lifecycle 的状态机、数据 owner、read model 和错误/cleanup contract。
- [ ] `design.md` 明确前端交互如何消费 AgentRun workspace state，以及哪些状态由后端投影负责。
- [ ] `implement.md` 给出分阶段执行计划，每个阶段有可验证结果、风险文件和验证命令。
- [ ] `implement.md` 包含测试矩阵，覆盖后端 application/API、contracts、frontend focused tests 与必要 grep 检查。
- [ ] `research/current-state.md` 保存本轮源码和历史任务证据索引，供实现阶段 agent 读取。
- [ ] `implement.jsonl` 和 `check.jsonl` 指向相关 spec 与 research 文件，供后续 Trellis sub-agent 使用。
- [ ] 任务保持 planning 状态，等待用户审阅后再 `task.py start`。

## Out Of Scope

- 本规划任务不直接重写启动链路代码。
- 本规划任务不删除 RuntimeSession trace/event 存储。
- 本规划任务不改变数据库 schema，除非后续实现阶段确认 launch plan 或 read model 需要持久化迁移。
- 本规划任务不重新设计 connector protocol。

## Open Questions

- 是否把 `AgentLaunchPlan` 做成显式持久化 read model，还是先作为 application 层 transient contract，并从现有 run / agent / anchor / frame facts 投影？推荐先以 application transient contract 收束代码边界；只有当前端恢复或失败诊断需要跨进程读取 launch plan 时再持久化。
