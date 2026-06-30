# Research: Local Runtime & Relay Surface

- Query: 单域对抗性架构审查：Local Runtime & Relay Surface；检查 relay wire protocol 与 local domain handlers 是否错层耦合，local command handlers 是否重新聚合成中央 hub，runner claim、desktop shell、local backend runtime 是否有重复事实源，并对照 06-14 baseline。
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/workflow.md` - Trellis 任务、研究与实现阶段约束。
- `.trellis/spec/index.md` - 规范索引，指向跨层、后端、会话与 desktop/local runtime 规范。
- `.trellis/spec/cross-layer/architecture.md` - 跨层分工与事件流边界。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - desktop shell、local runtime、runner claim 与 relay contract 的核心约束。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - project/backend/workspace routing 与 ProjectBackendAccess 分工。
- `.trellis/spec/backend/architecture.md` - 后端分层、runtime 状态与 relay gateway 归属。
- `.trellis/spec/backend/session/architecture.md` - session launch、placement、lease、relay connector 归属。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 06-14 基线汇总。
- `.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md` - 06-14 session/runtime 基线。
- `.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md` - 06-14 VFS/local relay/extension 基线。
- `crates/agentdash-relay/src/protocol.rs` - relay 顶层 envelope 与各 domain wire payload。
- `crates/agentdash-relay/src/protocol/mcp.rs` - MCP resolved transport wire contract。
- `crates/agentdash-relay/src/protocol/vfs_materialization.rs` - VFS materialization relay payload。
- `crates/agentdash-relay/src/protocol/extension_runtime.rs` - extension action/channel relay payload。
- `crates/agentdash-local/src/handlers/mod.rs` - local relay domain router assembly 与 dispatch。
- `crates/agentdash-local/src/handlers/prompt.rs` - local prompt relay handler、workspace prepare、prompt block conversion、session notification forwarder。
- `crates/agentdash-local/src/handlers/workspace.rs` - local workspace relay handler。
- `crates/agentdash-local/src/handlers/tool.rs` - local tool relay handler。
- `crates/agentdash-local/src/handlers/materialization.rs` - local materialization relay handler。
- `crates/agentdash-local/src/handlers/mcp.rs` - local MCP relay handler。
- `crates/agentdash-local/src/handlers/extension.rs` - local extension runtime relay handler。
- `crates/agentdash-local/src/handlers/terminal.rs` - local terminal relay handler。
- `crates/agentdash-local/src/runtime.rs` - LocalRuntimeManager、WebSocket config、relay backend runtime lifecycle。
- `crates/agentdash-local/src/desktop_runner_host.rs` - desktop host 对 LocalRuntimeManager 的 lifecycle bridge。
- `crates/agentdash-local/src/runner_claim.rs` - standalone runner claim client 与 credentials mapping。
- `crates/agentdash-local/src/tool_executor.rs` - local workspace root validation 与 tool execution boundary。
- `crates/agentdash-local/src/process_executor.rs` - local process cwd/root validation。
- `crates/agentdash-local-tauri/src/main.rs` - Tauri desktop shell commands、profile persistence、desktop ensure claim。
- `packages/core/src/local-runtime/index.ts` - frontend/core local runtime DTO 与 client abstraction。
- `packages/app-tauri/src/runtimeApi.ts` - Tauri invoke adapter。
- `packages/app-web/src/desktop/localRuntimeBridge.ts` - web-side desktop local runtime profile bootstrap。
- `crates/agentdash-application/src/backend/management.rs` - desktop ensure 与 runner enrollment 统一 use case。
- `crates/agentdash-application/src/backend/runner_registration.rs` - runner registration token claim。
- `crates/agentdash-api/src/routes/runner_registration_tokens.rs` - runner claim API route。
- `crates/agentdash-application-runtime-session/src/session/launch/planner.rs` - backend placement 与 lease claim。
- `crates/agentdash-application/src/relay_connector.rs` - cloud side relay connector、session route、lease activation/release、prompt block conversion。
- `crates/agentdash-api/src/relay/registry.rs` - relay registry、backend/session routes、MCP backend routing。
- `crates/agentdash-application/src/backend/runtime_summary.rs` - backend runtime summary projection。

### Code Patterns

- `LocalCommandRouter` is now a relay envelope router with per-domain handlers. The router fields are split by domain (`prompt`, `workspace`, `tool`, `materialization`, `mcp`, `extension`, `terminal`) in `crates/agentdash-local/src/handlers/mod.rs:49`, with dispatch remaining a `RelayMessage` match in `crates/agentdash-local/src/handlers/mod.rs:118`.
- Domain handlers own their domain dependencies rather than sharing one all-domain handler: `PromptCommandHandler` owns session/runtime/workspace forwarder concerns in `crates/agentdash-local/src/handlers/prompt.rs:20`; `MaterializationCommandHandler` only wraps `MaterializationStore` in `crates/agentdash-local/src/handlers/materialization.rs:7`; `TerminalCommandHandler` owns terminal process/session concerns in `crates/agentdash-local/src/handlers/terminal.rs:19`.
- MCP relay now carries resolved transport through `McpServerRelay { name, transport }` in `crates/agentdash-relay/src/protocol/mcp.rs:61`; cloud/local adapters preserve transport details in `crates/agentdash-application/src/mcp_relay_adapter.rs:6`; local connection keys include transport hash in `crates/agentdash-local/src/mcp_client_manager.rs:217`.
- VFS materialization remains a cloud plan plus local store split: relay payload carries plan/source/root/mount/cache data in `crates/agentdash-relay/src/protocol/vfs_materialization.rs:4`, while local handler delegates to `MaterializationStore` in `crates/agentdash-local/src/handlers/materialization.rs:18`.
- Runner claim is server-issued identity: local runner claim maps `/api/local-runtime/runner/claim` response into credentials in `crates/agentdash-local/src/runner_claim.rs:51` and `crates/agentdash-local/src/runner_claim.rs:122`; server enrollment centralizes backend identity in `crates/agentdash-application/src/backend/management.rs:195`.
- Runtime placement uses backend selection plus lease as execution truth: launch planner creates `BackendExecutionLease::claimed` in `crates/agentdash-application-runtime-session/src/session/launch/planner.rs:315`; relay connector requires `context.session.backend_execution` in `crates/agentdash-application/src/relay_connector.rs:103`; registry routes by backend/session route in `crates/agentdash-api/src/relay/registry.rs:233`.

### Issues

#### LR-1 - Desktop shell still owns desktop ensure/profile contract instead of delegating it to `agentdash-local`

- Priority: P1
- Classification: 重复事实源 / desktop shell 职责漂移 / local runtime ownership gap
- Code evidence:
  - Tauri shell defines `RuntimeStartRequest`, `LocalRuntimeProfile`, `EnsureLocalRuntimePayload`, and `EnsureLocalRuntimeResponse` in `crates/agentdash-local-tauri/src/main.rs:107`, `crates/agentdash-local-tauri/src/main.rs:124`, `crates/agentdash-local-tauri/src/main.rs:423`, and `crates/agentdash-local-tauri/src/main.rs:445`.
  - Tauri shell persists profile JSON directly in `profile_load` / `profile_save` at `crates/agentdash-local-tauri/src/main.rs:244` and `crates/agentdash-local-tauri/src/main.rs:256`.
  - Tauri shell performs the desktop ensure HTTP flow directly: `start_runtime_from_request` calls `claim_local_runtime` at `crates/agentdash-local-tauri/src/main.rs:638`; `claim_local_runtime` builds the ensure payload at `crates/agentdash-local-tauri/src/main.rs:662`; `post_local_runtime_claim` posts `/api/local-runtime/ensure` at `crates/agentdash-local-tauri/src/main.rs:752`.
  - The same conceptual DTO also exists in frontend core as `RuntimeStartRequest` and `LocalRuntimeProfile` at `packages/core/src/local-runtime/index.ts:197` and `packages/core/src/local-runtime/index.ts:208`.
  - `agentdash-local` runtime consumes already-claimed config via `LocalRuntimeConfig` in `crates/agentdash-local/src/runtime.rs:29`; it does not own desktop profile normalization or desktop ensure claim.
- Impact:
  - Desktop profile shape, machine identity normalization, ensure request shape, and claim response validation are split across frontend core, Tauri shell, server application code, and local runtime startup.
  - The local runtime library is not the canonical desktop enrollment/profile owner, while the standalone runner has an explicit `agentdash-local` claim client in `crates/agentdash-local/src/runner_claim.rs:51`.
  - Any future change to backend enrollment fields or profile semantics must coordinate shell DTOs and local runtime config manually.
- Convergence boundary:
  - Move desktop profile load/save/normalize, desktop ensure payload/response, claim validation, and HTTP claim client into `agentdash-local` as the canonical desktop-local-runtime module.
  - Keep `agentdash-local-tauri` as a Tauri command adapter that passes UI input to `agentdash-local` and starts/reuses `LocalRuntimeManager` through library APIs.
  - Keep server-issued `backend_id`, `relay_ws_url`, and token as the only formal identity source.
- 06-14 baseline:
  - This is the same residual concern as 06-14 P2 “Tauri `main.rs` reimplemented profile/claim and should move profile/claim down to `agentdash-local`”. The server-side enrollment path improved, but the desktop shell still contains the desktop claim/profile implementation.

#### LR-2 - Workspace root validation is duplicated between `ToolExecutor` and `ProcessExecutor`

- Priority: P2
- Classification: duplicated local execution boundary / repeated workspace-root fact source
- Code evidence:
  - `ToolExecutor` stores `workspace_roots_configured` and `canonical_workspace_roots` in `crates/agentdash-local/src/tool_executor.rs:20`, then validates roots in `crates/agentdash-local/src/tool_executor.rs:80`.
  - `ProcessExecutor` stores the same configured/canonical root facts in `crates/agentdash-local/src/process_executor.rs:23`, then repeats equivalent validation in `crates/agentdash-local/src/process_executor.rs:38`.
  - `ProcessExecutor::resolve_cwd` depends on its own root validation before resolving cwd in `crates/agentdash-local/src/process_executor.rs:71`.
  - Terminal spawn validates `mount_root_ref` through `ToolExecutor` before calling shell sessions in `crates/agentdash-local/src/handlers/terminal.rs:35`.
- Impact:
  - File/tool operations and process/shell operations rely on two validators that must remain semantically identical.
  - The current behavior is aligned, but the invariant is maintained by duplicated code rather than by one shared boundary object.
  - Drift here would directly affect terminal, shell execution, file tools, extension-host spawned processes, and any future local command that accepts cwd/root input.
- Convergence boundary:
  - Introduce one `WorkspaceRootGuard` / workspace boundary value inside `agentdash-local` and inject it into `ToolExecutor`, `ProcessExecutor`, and terminal/session helpers.
  - Keep canonicalization and “explicit root vs configured roots” semantics in that one object; domain handlers should consume the result rather than own or duplicate root policy.
- 06-14 baseline:
  - Not called out in the 06-14 baseline. It is a smaller local runtime duplicate-fact issue that remains after the larger command-hub refactor.

#### LR-3 - Relay prompt input still crosses the boundary as ACP `ContentBlock` JSON with lossy paired conversions

- Priority: P2
- Classification: relay wire abstraction leak / protocol payload gap
- Code evidence:
  - Cloud-side relay connector still documents that the relay boundary uses ACP `ContentBlock` JSON and that non-text content may degrade in `crates/agentdash-application/src/relay_connector.rs:112`.
  - Cloud conversion is implemented as `user_input_blocks_to_relay_prompt_blocks` with format-specific behavior in `crates/agentdash-application/src/relay_connector.rs:433`.
  - Local prompt handler converts `payload.prompt_blocks` back into canonical `UserInputBlock` values in `crates/agentdash-local/src/handlers/prompt.rs:187`.
  - Local conversion parses raw JSON into `ContentBlock` and maps unsupported cases into text fallback in `crates/agentdash-local/src/handlers/prompt.rs:362`.
- Impact:
  - Prompt payload semantics are split between cloud connector and local prompt handler instead of being fully owned by the relay protocol type.
  - Data URL images round-trip, but remote/local images, skill blocks, and mentions can degrade before reaching local runtime execution.
  - The two conversion helpers must evolve in lockstep, making prompt input capabilities harder to reason about than typed workspace/tool/MCP/materialization payloads.
- Convergence boundary:
  - Define a typed relay prompt input payload that reflects the canonical user input contract needed by the runtime boundary, or generate the relay payload from the same canonical type used by session launch.
  - Keep ACP-specific conversion at one edge instead of pairing raw JSON conversion in both cloud connector and local prompt handler.
- 06-14 baseline:
  - 06-14 did not flag this as part of the local command-hub issue. Current code comments already identify this as an interim remote-boundary limitation; it is now the main remaining relay wire/protocol coupling in this surface.

### Non-Issues / Resolved Against Baseline

- Local command handlers have not re-formed the 06-14 central hub. `LocalCommandRouter` is still broad at construction time, but it only routes `RelayMessage` variants to domain handlers in `crates/agentdash-local/src/handlers/mod.rs:118`; the domain-specific logic is in separate handlers.
- Top-level `RelayMessage` remains an envelope and is still an acceptable central contract, matching the 06-14 baseline that the enum itself was not the issue.
- MCP resolved transport now matches the spec expectation: relay payload carries full `McpServerRelay` transport data in `crates/agentdash-relay/src/protocol/mcp.rs:61`, adapter conversion is centralized in `crates/agentdash-application/src/mcp_relay_adapter.rs:6`, and local protect-mode plus transport-specific connection reuse live in `crates/agentdash-local/src/mcp_client_manager.rs:149`.
- VFS materialization remains intentionally split between cloud-side materialization plan and local materialized store; no new centralized hub was found in `crates/agentdash-local/src/handlers/materialization.rs:18`.
- Extension root override and output schema concerns from 06-14 are substantially addressed: host API rejects workspace root overrides in `crates/agentdash-local/src/extensions/host/host_api.rs:112`, local extension manager validates action/channel output schema in `crates/agentdash-local/src/extensions/host/manager.rs:115`, and gateway validates extension action input before transport in `crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:169`.
- Runner claim does not duplicate formal backend identity locally. Runner credentials are mapped from server response in `crates/agentdash-local/src/runner_claim.rs:122`; backend enrollment identity is generated server-side in `crates/agentdash-application/src/backend/management.rs:195`; project visibility is handled through access grants in `crates/agentdash-application/src/backend/runner_registration.rs:177`.
- Local backend runtime placement has multiple projections but not multiple owners: backend config/enrollment, relay online snapshot, and execution leases are read together in runtime summary at `crates/agentdash-application/src/backend/runtime_summary.rs:52`; execution truth for a prompt is the claimed lease and registered relay session route.

### Related Specs

- `.trellis/spec/cross-layer/desktop-local-runtime.md` - Declares Tauri as a thin shell, `agentdash-local` as machine identity/runtime owner, and server claim response as source of `backend_id`, `relay_ws_url`, and relay auth token.
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - Separates backend identity from project access and workspace routing.
- `.trellis/spec/backend/session/architecture.md` - Places backend execution lease and relay connector behavior in session launch/runtime flow.
- `.trellis/spec/backend/architecture.md` - Keeps backend enrollment and runtime state in application/backend services, with relay API as transport boundary.
- `.trellis/spec/cross-layer/architecture.md` - Requires cross-layer contracts to have one owner rather than reimplementing policy in shell/adapters.

### External References

- None. This review used repository code, Trellis specs, and 06-14 baseline research only.

## Caveats / Not Found

- No business code was modified.
- No full test suite was run, per task instruction.
- The Trellis active-task helper reported no active task, so this research used the user-provided task path `.trellis/tasks/06-30-module-adversarial-review` and wrote only to the explicitly allowed research file.
- Line references are from the current working tree during review; concurrent edits may shift exact line numbers.
