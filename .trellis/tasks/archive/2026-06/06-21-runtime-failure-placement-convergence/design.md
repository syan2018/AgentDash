# Runtime Failure And Placement Design

## Problem Shape

执行期 backend 缺失、disconnect、MCP fallback 与 local backend identity 目前没有统一产品语义。需要先 characterization，再决定 projection / fallback 收敛。

## Placement Boundaries

| Context | Target Rule |
| --- | --- |
| session execution | 强制 session route / backend execution |
| setup / probe | 可以使用 catalog / discovery fallback |
| VFS utility | 绑定 mount backend/root |
| terminal | mount utility；可通过 completion outbox 回调进入 AgentRun steer / turn-boundary，但不占 execution lease |

## Local Backend Identity

standalone `agentdash-local` 是已领取 backend identity 的消费入口：`backend_id` 必须来自 server ensure/claim response，或由调用方显式传入与 relay token 绑定的一致值。这样本机 runtime 只持有机器身份事实，backend identity 仍由 server claim 约束。

## Failure Projection

- backend disconnect / execution backend missing 是独立 lost 终态，不映射为 completed、failed 或 interrupted。
- session terminal event 使用 `turn_lost`，delivery / runtime projection status 使用 `lost`。
- backend disconnect cleanup 必须先持久化或投递 lost terminal，再清理 session route / sink 和 lease，原因是 route 先删除会让 connector stream close 被解析成 completed。
- runtime-summary、feed、AgentRun shell、session route cleanup 必须一致；lost lease 不再作为 active session，但 feed / AgentRun 必须保留 lost diagnostic。

## MCP Fallback

- session context 下 MCP list/call 是 session-route-bound command。
- session route 缺失或 route backend 离线时直接失败，不 fallback 到 VFS default mount、advertised catalog 或任意在线 backend。
- setup/probe 是 setup-bound command，继续允许 catalog / discovery fallback。
