# Complete Agent 当前持久化模型证据

## Reproduction

命令：

```powershell
pnpm run dev:server
```

稳定结果：

```text
Error: Complete Agent host invariant failed:
service instance id is already registered with different verified facts
```

只启动 migrate + `agentdash-server serve` 即可复现，尚未发生 UI 请求或 Managed profile 选择，
因此触发者位于启动期静态 Complete Agent contribution。

## Database Evidence

当前开发数据库的 Complete Agent inventory 只有：

```text
service_instance_id   = builtin.codex-app-server.default
definition_id         = builtin.codex-app-server
placement_kind        = in_process
host_incarnation_id   = agentdash-api-host-04feaa2e-f251-4859-88e6-ad2f996c0904
publisher_integration = builtin.codex_runtime
service_version       = 144
```

当前源码 `CODEX_APP_SERVER_PROTOCOL_REVISION` 同为 `144`，排除协议版本、descriptor 与 build claim
自然升级导致的冲突。数据库没有 `builtin.dash-agent.*`，排除 Native 动态 instance ID 碰撞。

## Contradictory Invariants

1. `crates/agentdash-api/src/app_state.rs:433`
   每次启动通过 `Uuid::new_v4()` 创建新的 Host incarnation。
2. `crates/agentdash-agent-runtime-host/src/complete_agent.rs:396-408`
   同一个 service instance 重注册时要求 descriptor、placement、verification 与 remote mapping
   全部相等。
3. `crates/agentdash-agent-runtime-host/src/complete_agent_repository.rs:247-253`
   要求每个 service instance 恰有一个 offer、placement 与 verification。
4. `crates/agentdash-agent-runtime-host/src/complete_agent_repository.rs:323-336`
   将 descriptor、offer、placement 与 verification 全部声明为不可变历史。
5. `crates/agentdash-infrastructure/migrations/0084_agent_runtime_complete_agent_hard_cut.sql:984-994`
   `agent_runtime_placement` 以 `service_instance_id` 为主键，同时保存
   `host_incarnation_id`。

结果是“incarnation 每次启动必须变化”和“placement 一旦持久化永远不能变化”同时成立，正常重启
必然冲突。

## Unsafe Patch Warning

`CompleteAgentBinding` 当前只保存 `service_instance_id + generation + source + profile/surface`，
没有 live attachment 或 Host incarnation。Host dispatch 又只按 `service_instance_id` 从进程 registry
解析 handle。

因此不能通过允许原地更新 placement 修复启动：那会让旧 binding/generation 在重启后悄悄命中新
进程 handle，破坏 stale generation/incarnation fencing。

同样不能通过复用 incarnation、启动清库、吞掉 invariant、给 instance ID 拼随机 UUID 或把 Codex
注册错误当普通 unavailable 解决；这些方案会分别破坏 fencing、durability、完整性诊断或 recovery
identity。

## Persistence Classification

不应持久化：

- 当前 service descriptor / verification result / offer；
- live service handle 与 current availability；
- placement、connection epoch 与 Host incarnation 作为全局当前 inventory；
- Native adapter cache 与 recovery selection catalog。

应持久化：

- canonical Runtime target 与 binding generation；
- exact binding target snapshot；
- source coordinate、callback route 与 surface/hook applied evidence；
- effect identity、payload digest、receipt/inspection evidence 与 outbox；
- lease owner/token/epoch/expiry；
- recovery 的 previous/current target 与 generation 证据。

历史 service/build/profile/placement 坐标只有在某个 binding/effect 已经选择它们后才成为 durable
事实，并应冻结在该 binding target snapshot 中。
