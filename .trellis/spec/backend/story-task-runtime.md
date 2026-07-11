# Story / Task Runtime Projection

Story 与 Task 拥有业务内容、状态和 subject association；它们不拥有 Agent execution lifecycle。执行投影由 LifecycleRun/Agent/Frame、Workflow node evidence 与 canonical Runtime thread/operation/snapshot组合。

## Contracts

- Subject execution 创建 Lifecycle product facts 后经 `AgentRunProductDelivery` 进入 Runtime。
- Task/Story 不保存 Runtime status、transcript、context head 或 Driver source ID。
- UI status projection 读取 Workflow/Lifecycle 产品进度；需要执行详情时按 `AgentRunRuntimeTarget` 调用 facade。
- artifact/result 必须引用明确 node attempt 与 Runtime operation evidence。
- duplicate/late terminal event 通过 operation identity 与 attempt coordinate 幂等处理。

## Tests Required

- Subject dispatch 返回 run/agent/frame 和 canonical thread/operation refs。
- Runtime Lost/failed/completed 投影不重复推进 Task/Story。
- 跨 Project subject 无法读取 Runtime target。
