# W7 — Product & Protocol Cutover

## Depends On

- W3 Runtime State & Host Coordination
- W5 Native / Dash Adapter
- W6 Codex / Remote Adapters

## Ownership

- `agentdash-application-agentrun`
- Companion product modules
- Agent Runtime API/App Server projection
- generated frontend contracts and Agent UI state
- Application-owned `AgentRunForkSaga` repository/state machine

## Goal

让 AgentRun、Fork、Companion、API、App Server protocol 和 UI 全量消费统一 Runtime
command/snapshot/change，不再从 presentation journal、worker timing 或 vendor state 推断。

## Exit Criteria

- AgentRun 只依赖 Runtime Contract；
- Fork 使用 Application-owned durable saga + Runtime/Host provisioning + native Agent fork
  + product graph commit + explicit activation；
- 任意 crash/unknown outcome 继续同一 effect/child，Lost 保留 child coordinate；
- Companion `Full` exact fork history，其余 slice fresh typed context；
- fresh slice 编译为平台中立 `InitialAgentContextPackage`，create receipt 证明
  digest/fidelity 后才激活 child，派发任务随后作为首个普通 input；
- `adoption_mode` 与 history 创建方式正交；
- visible child history 不依赖 ancestor current journal；
- App Server/UI lifecycle 从 committed Runtime change 投影；
- cursor gap 使用 snapshot reload。
