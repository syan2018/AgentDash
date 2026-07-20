# Agent Runtime Context

AgentDashboard 通过 Product、Managed Runtime、Complete Agent Host 与具体 Complete Agent
四个 owner 表达 AgentRun 的产品生命周期、平台协调、执行绑定和 Agent 自有历史。每个 owner
拥有自己的 canonical fact graph，并以 typed identity、revision 与 digest 交换证据。

## Language

**AgentRun Product**:
面向用户与工作流的产品 aggregate，拥有 AgentRun、Frame、Mailbox、Routine、Workflow、
Companion、资源快照以及 Runtime change 的 Product 消费进度。

**Managed Runtime**:
平台对 `RuntimeThread` 的协调 aggregate，拥有 source binding、normalized projection、
command admission、Operation、Change 与 Outbox。它投影 Complete Agent 事实，但不拥有具体
Agent 的原生历史或上下文。

**Complete Agent**:
实现完整 Agent 语义的服务边界。它提供 snapshot/change、command receipt、fork、context 与
compaction 能力；Dash Complete Agent、Codex Complete Agent 和企业 Agent 都通过同一协议
接入。

**Complete Agent Host**:
平台内承载 Complete Agent live attachment 的执行边界，拥有 verification、exact target、
binding/generation、callback route、effect、lease 与 recovery evidence。当前进程的可调用
handle 与 availability 位于 live catalog，durable Host graph 位于 Host owner document。

**Concrete Agent State**:
具体 Complete Agent 自己拥有的 session/branch/history/context/compaction 状态。不同 Agent
实现可以采用不同内部模型，只需在 Complete Agent 协议边界提供经过验证的事实与能力。

**RuntimeThread**:
Managed Runtime 使用的稳定协调 identity。AgentRun Product binding、Host runtime target 与
具体 Agent source coordinate 通过 typed evidence 与该 identity 关联。

**Managed Runtime Operation**:
Managed Runtime 对一个 command 的 durable acceptance、idempotency identity、执行状态与
terminal evidence。

**Managed Runtime Change**:
Managed Runtime transaction 提交的有序平台变更。Change 与待消费 Outbox 同属 Runtime owner
document；Product consumer 在自己的 binding document 中保存独立 delivery cursor。

**Owner Document**:
一个 aggregate 的 canonical `revision + facts JSONB` 持久化形态。完整 fact graph 在 domain
边界校验并通过 revision CAS 原子提交，适合需要整体单调性、幂等和恢复的 Runtime/Host/
Callback 状态。

**Cross-owner Evidence**:
一个 owner 为完成自身业务而冻结的外部 identity、revision、generation、digest 或 receipt。
证据允许验证另一 owner 的事实版本，同时保持双方独立的持久化与演进边界。
