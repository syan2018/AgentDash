# S5 production composition and final deletion

## Purpose

本文固定 S5 后半段真正剩余的 production activation、Platform SPI 收窄与 legacy
protocol/types/codegen 删除顺序。审计基于 hard-cut staging `84caddb4`。

当前 blocker 不是继续“删除 Runtime journal”：

- `RuntimeJournalFact`、`RuntimeJournalRecord`、`journal_records_after` 与
  `append_presentation*` 已全树零命中；
- 0045 已迁名无前缀 `session_*`，0065/0084 删除了迁名后表；
- 0070 的 Runtime journal 实体 `agent_runtime_event` 已由 0084 删除；
- migration readiness 已有 REQUIRED/RETIRED 双向门禁。

剩余问题是 final implementations 尚未进入唯一 production composition，真实 consumers
尚未切换，因而 P5 物理删除仍不可执行。

## Platform Tool / Hook activation

### Final implementation exists

- `agentdash-infrastructure/src/runtime_tool_executors.rs`
  - `final_runtime_tool_catalog`
  - `mounts_list`、`fs_read`、`fs_glob`、`fs_grep`、`fs_apply_patch`、`shell_exec`、
    `task_read`、`task_write`
- `agentdash-infrastructure/src/runtime_tool_authorization.rs`
  - `ProductRuntimeToolAuthorizer`
  - `RuntimeToolProductBindingQueryPort`
- `agentdash-agent-runtime-host/src/runtime_tool_handler.rs`
  - `RuntimePlatformToolHandler`
- `agentdash-agent-runtime/src/platform_tool_broker.rs`
  - `RuntimeToolBroker`

### Production activation is missing

Production currently constructs none of:

- `RuntimeToolBroker::new`;
- `final_runtime_tool_catalog(...)`;
- `ProductRuntimeToolAuthorizer::new`;
- `AppliedVfsRuntimeToolService::new`;
- `ApplicationRuntimeTaskToolService::new`;
- `RuntimePlatformToolHandler::new`.

`agentdash-api/src/app_state.rs` still injects `UnsupportedAgentNativeCallbacks` for both Tool and
Hook, so a real Agent callback is rejected even though owner tests pass.

Required closure:

1. PostgreSQL implements `RuntimeToolProductBindingQueryPort`.
2. Product binding persistence proves `binding_digest`、
   `applied_resource_snapshot_revision` and `applied_resource_binding_generation`.
3. Materialize-before-activation atomically pins the immutable resource surface and current Host
   binding generation.
4. Infrastructure constructs VFS/Task services、final catalog、Product authorizer and Broker.
5. Host constructs `RuntimePlatformToolHandler` over the Broker.
6. Complete Agent composition receives real Tool and Hook callbacks.
7. Production tests prove authorization、replay、deadline、stale generation and receipt-loss
   inspection.

### VFS and Task physical cut

`agentdash-application-vfs/src/runtime_tools.rs` is final and reuses concrete executors under
`src/tools/**`. The final cut extracts executor、schema、execution state and terminal registry into
a VFS-owned neutral module, then deletes only `AgentTool` wrappers、factory and legacy
result/update adapters. A real production `ShellTerminalRegistry` implementation is required.

`agentdash-application/src/task/tools.rs` mixes final
`ApplicationRuntimeTaskToolService` with legacy `TaskReadTool`/`TaskWriteTool`. Split the final
service first; after Tool composition activates, delete `task/runtime_tool_provider.rs`、
`task/scope.rs` and legacy tool/result helpers.

The old provider catalog also exposed Collaboration、Workflow、Workspace and Wait. Product/
Platform must prove each has a final typed route or an explicit final product decision before its
legacy implementation is removed. Absence from the new eight-tool catalog is not deletion
evidence.

## Platform SPI final shape

Keep only modules with an independent non-Agent adapter reason:

- auth;
- function runner;
- MCP injection/probe/relay;
- mount/VFS discovery;
- memory/skill discovery/source;
- marketplace;
- routine;
- extension package;
- workflow script;
- platform tool capability policy.

Remove or migrate:

- `platform/runtime_surface.rs` Agent execution/session/turn/prompt/tool-provider types;
- `session_persistence.rs`;
- `hooks/trace.rs`;
- Backbone/protocol/session provenance and old Agent callback vocabulary from `hooks/mod.rs`;
- Agent types、`AgentTool`/`DynAgentTool`、runtime/session re-exports from `lib.rs`.

`session_persistence.rs` is business-zero except `SessionStoreError` references in AgentRun、
Workflow and API RPC error mapping. Move those call sites to their owner errors, then delete the
module.

Hook trace projector/storage disposition is production-zero, but Lifecycle still consumes the old
Hook runtime trait. Product must first migrate that behavior to final Hook contributions and
`CompleteAgentHookHandler`; deletion cannot remove the product capability.

## Complete Agent production registration

### Native

`NativeCompleteAgentIntegration` exists, but first-party builtins and the API verifier currently
register only Codex. The External/Dash owner must provide:

- Native contribution registration;
- trusted verification record;
- durable store/service construction;
- production Complete Agent conformance evidence.

W8 only composes the checked owner contribution.

### Remote

`RemoteCompleteAgentIntegration` and `RemoteCompleteAgentService` exist, but API/Local/Relay has
no production `RuntimeWirePlacement`. Implement and verify the placement path described in
[`relay-runtime-wire-placement-activation.md`](relay-runtime-wire-placement-activation.md) before
Remote registration and before deleting Relay's old Agent lane.

## Relay legacy lane

Old zero-execution protocol remnants:

- `CommandPrompt`、`CommandCancel`、`CommandSteer`;
- `ResponsePrompt`、`ResponseCancel`、`ResponseSteer`;
- `EventSessionNotification`;
- `EventRuntimeSessionStateChanged`;
- `protocol/prompt.rs`、`protocol/session_event.rs`;
- registry labels and generic response routing.

They are deleted only after placement implements advertisement/open/frame/ack/closed/lost,
trusted verification, Local endpoint catalog, stream pump and dynamic register/detach. Workspace、
Terminal、MCP、Extension and VFS Relay lanes remain.

## W8 production composition

`AgentRunProductPersistenceComposition` exists but has no production `build(...)` call.
`AppState` currently composes only Product projection query.

The final atomic composition bundle must construct:

1. Product persistence composition;
2. applied-resource materialize and activation pin;
3. Product Runtime command and mailbox facades;
4. Tool authorizer/Broker/catalog/handler;
5. real Hook handler;
6. Native contribution and verifier;
7. trusted Remote registrar/verification/placement;
8. final repositories and recovery/outbox workers.

No owner may introduce an in-memory production fallback to make this graph compile.

## Protocol/types/codegen consumer cut

### Product P3 consumers

The following remain real until
[`product-canonical-presentation-cutover.md`](product-canonical-presentation-cutover.md) is
complete:

- Application Ports launch/projection notification;
- AgentRun thread-name/Product protocol;
- Lifecycle relation writer;
- API Project projection notification;
- Contracts session/generator/project/mailbox/project-agent/canvas roots;
- browser history/feed/tool-card consumers of `backbone-protocol.ts`.

W8 cannot delete or rewrite their semantics on behalf of Product.

### Zero-consumer cleanup candidates

After final capability-route evidence:

- manifest-only Protocol deps in Workflow、VFS and Workspace Module;
- direct Agent Types deps used only by unmounted Application/Lifecycle files;
- no-caller task lifecycle envelope;
- no-caller Hook trace projector;
- Relay old Agent variants after placement activation;
- unmounted legacy Application frame/companion/routine/runtime_tools/wait/VFS owner files;
- unmounted Lifecycle VFS/surface/tools files after Product history migration;
- unmounted Workspace Module runtime-context/bridge/tool/surface files after Product route proof.

An unmounted file is not automatically a removable feature. Collaboration、Workflow、Workspace
and Wait behavior must first have a final route or explicit final product decision.

## Final migration additions

0084 already deletes the old Runtime/session/journal tables. Add only final facts:

- Product Runtime binding digest;
- applied-resource snapshot revision pin;
- Host binding generation pin;
- CAS/activation transaction tying those values together;
- `RuntimeToolProductBindingQueryPort` PostgreSQL adapter and validation.

Old Product command/mailbox persistence can be physically split only after P3 switches production
callers. `PostgresAgentRunMessageSubmissionStore` has no production constructor and is an early
deletion candidate after that cut.

## Activation and deletion order

1. Integrate checked canonical presentation P1/P2.
2. Complete Product P3 consumers and freeze old protocol real consumers at zero.
3. Platform activates Tool/Hook execution primitives、PG authorizer and production callbacks.
4. External/Dash activates Native contribution and verifier.
5. Relay activates Runtime Wire placement and Remote contribution over a real WebSocket.
6. W8 composes Product persistence、Tool/Hook、Native、Remote and recovery.
7. Delete zero-consumer source and manifest-only dependencies.
8. Narrow Platform SPI.
9. Switch root generation/check to Codex private plus Runtime/Service/Wire/Product generators.
10. Delete `agentdash-agent-protocol-codegen`、`agentdash-agent-protocol`、
    `agentdash-agent-types` and browser/vendor legacy generated artifacts.
11. Finalize 0084 constraints/repositories/schema guard.
12. Generate the sole final `Cargo.lock`.
13. Run architecture/behavior checks and all tracer bullets on one staging tip.

## Required gates

```powershell
pnpm migration:guard
pnpm test-support:guard
pnpm contracts:generate
pnpm contracts:check
cargo test -p agentdash-agent-runtime
cargo test -p agentdash-agent-runtime-host
cargo test -p agentdash-integration-native-agent
cargo test -p agentdash-integration-codex
cargo test -p agentdash-integration-remote-runtime
cargo test -p agentdash-application-vfs
cargo test -p agentdash-application-agentrun
cargo test -p agentdash-relay
cargo test -p agentdash-infrastructure
cargo test -p agentdash-api
pnpm --filter app-web typecheck
pnpm --filter app-web test
```

```powershell
rg -n "RuntimeJournalFact|RuntimeJournalRecord|journal_records_after|append_presentation" crates packages
rg -n "BackboneEvent|BackboneEnvelope|backbone-protocol" crates packages
rg -n "AgentTool|DynAgentTool|RuntimeToolProvider|AgentRuntimeDelegateSet" crates/agentdash-platform-spi crates/agentdash-application crates/agentdash-application-vfs crates/agentdash-application-lifecycle crates/agentdash-workspace-module
rg -n "CommandPrompt|CommandCancel|CommandSteer|EventSessionNotification|EventRuntimeSessionStateChanged" crates/agentdash-relay crates/agentdash-api crates/agentdash-local
rg -n "agentdash-agent-types|agentdash-agent-protocol|agentdash-agent-protocol-codegen" Cargo.toml crates package.json
rg -n "UnsupportedAgentNativeCallbacks" crates/agentdash-api
```

`cargo metadata --format-version 1` must prove the three legacy crates have zero normal direct
dependents. Real tracers must cover Tool/Hook callback、Remote placement/replay/reconnect/API
restart、direct input/output、Fork、Companion、Compaction、Runtime reconnect and every non-Agent
Relay lane.
