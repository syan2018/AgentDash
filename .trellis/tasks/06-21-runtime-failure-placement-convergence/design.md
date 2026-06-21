# Runtime Failure And Placement Design

## Problem Shape

执行期 backend 缺失、disconnect、MCP fallback 与 local backend identity 目前没有统一产品语义。需要先 characterization，再决定 projection / fallback 收敛。

## Placement Boundaries

| Context | Target Rule |
| --- | --- |
| session execution | 强制 session route / backend execution |
| setup / probe | 可以使用 catalog / discovery fallback |
| VFS utility | 绑定 mount backend/root |
| terminal | 待确认：mount utility 或 execution surface |

## Local Backend Identity

standalone `agentdash-local` 是已领取 backend identity 的消费入口：`backend_id` 必须来自 server ensure/claim response，或由调用方显式传入与 relay token 绑定的一致值。这样本机 runtime 只持有机器身份事实，backend identity 仍由 server claim 约束。

## Failure Projection

- backend disconnect for running execution should produce user-visible lost / terminal projection if confirmed by design.
- runtime-summary、feed、AgentRun shell、session route cleanup 必须一致。
