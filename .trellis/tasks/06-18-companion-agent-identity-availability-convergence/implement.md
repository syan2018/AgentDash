# Implementation Plan

## Phase 0: Context

- Read backend capability spec and update it after implementation:
  - `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- Re-read relevant source before editing:
  - `crates/agentdash-domain/src/common/agent_config.rs`
  - `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs`
  - `crates/agentdash-application/src/companion/tools.rs`
  - `crates/agentdash-application/src/session/construction_provider.rs`
  - `crates/agentdash-application/src/session/assembler.rs`
  - `crates/agentdash-application/src/agent_run/frame/construction/composer_companion.rs`
  - `packages/app-web/src/features/project/agent-preset-editor/`

## Phase 1: Availability Model

- Add target-side default companion field to `AgentPresetConfig` and merge logic.
- Replace caller-side whitelist semantics with additive extra companions semantics.
- Update companion candidate loading:
  - include default-enabled sibling Agents;
  - include caller extra companions;
  - exclude caller itself;
  - deduplicate by canonical agent key.
- Update tests around roster rendering and capability projection.

## Phase 2: Frontend Configuration

- Update `AgentPresetConfig` TypeScript type and generated/derived contract usage.
- Update preset form state conversion.
- Add target-side control for default companion availability.
- Update companion picker to list only non-default sibling Agents and persist extra companions.
- Update project agent card indicators so they distinguish “default companion” from “extra companion target”.
- Remove old whitelist-mode copy and empty-list semantics from ProjectAgent UI.
- Update AgentRun / companion dispatch UI models so selected ProjectAgent identity is displayed for child companion sessions.
- Ensure context frame / roster delta rendering remains backend-driven and does not locally infer companion availability.

## Phase 3: Companion Launch Identity

- Change `CompanionRequestTool` selected-agent resolution to return ProjectAgent identity plus preset context, not only `AgentConfig`.
- Extend companion launch source / dispatch metadata to carry selected ProjectAgent identity.
- Ensure child LifecycleAgent or equivalent runtime surface records selected ProjectAgent identity.
- Refactor companion frame construction so it can combine parent context slice with selected ProjectAgent preset facts.
- Preserve companion-system skill and companion response return-channel behavior.
- Audit collaboration capability checks so launch guard and result-return channel are not accidentally coupled.

## Phase 3.5: Operation Surface Boundary

- Inventory current uses of `ToolCapabilityPath` as both exposure directive and permission grant path.
- Decide the minimal representation for non-escalatable built-in operation surfaces / guards:
  - first-class operation/guard IDs; or
  - frame capability/context construction facts that tool execution also consumes.
- Design the minimal `AuthorityState` / operation authority projection needed for companion without forcing the whole app to migrate at once.
- Implement the direction as `AuthorityState -> CapabilityState`, not peer systems. Capability projection should consume authority decisions to crop companion roster and workspace module presentation affordances.
- Keep guard checks separate from PermissionGrant / capability grant flows.
- Ensure companion roster projection is suppressed when `companion.dispatch` is unavailable.
- Ensure workspace module presentation/display affordance is cropped for subagent identity and remains available to user-invoked main/root ProjectAgent sessions.
- Ensure `companion_request(target=sub)` can be denied without removing `companion_respond` from child sessions.
- Ensure companion child runs default-deny `companion_request(target=human)` unless the user has actively messaged/entered that companion run.
- Capture dynamic workflow as a motivating guard case: authoring/launching generated orchestration should be limited to main/root ProjectAgent sessions unless explicitly changed later.
- Add tests for a child companion retaining return-channel access when parent launch permission is restricted.
- Add tests for companion child human-route suppression and user-activated companion run human-route enablement.

## Phase 4: Data And Contracts

- Add database migration if persisted config/index schema needs direct shape changes.
- Update seed/mock/sample ProjectAgent configs if present.
- Regenerate frontend contracts if this repo's contract generation flow requires it.
- Update `.trellis/spec/backend/capability/tool-capability-pipeline.md` with the new availability and launch identity contracts.

## Validation

- Rust focused tests:
  - `cargo test -p agentdash-application companion`
  - `cargo test -p agentdash-application capability`
  - targeted tests for ProjectAgent start / companion dispatch if narrower names exist.
- Frontend focused tests:
  - `pnpm --filter @agentdash/app-web test -- project`
  - targeted preset form tests if available.
- Final lightweight check:
  - `cargo check -p agentdash-application`
  - `pnpm --filter @agentdash/app-web typecheck`

## Review Gates

- Before implementation starts, decide how this task represents non-escalatable operation surfaces separately from tool exposure and PermissionGrant.
- Before finishing, verify that a model-visible `agent_key` cannot launch a child whose identity differs from the roster entry.

## Current Implementation Notes

- `allowed_companions` 已替换为目标侧 `default_companion_enabled` 与调用侧 `extra_companions`；roster 规则为 default-enabled siblings ∪ caller extras - self。
- `companion_request(payload.agent_key)` 现在必须选择当前 roster 中的 ProjectAgent；selected identity 会进入 dispatch result、launch source、child `LifecycleAgent.project_agent_id` 与 frame construction。
- selected companion child 在 parent slice 上叠加 selected ProjectAgent executor config、capability directives、MCP presets、VFS grants 与 skill assets。
- `AuthorityState` 已接入 resolver；main ProjectAgent 保留 dispatch / human / workspace module / dynamic workflow authoring，companion child 隐藏 dispatch / human / workspace module，拒绝 dynamic workflow authoring，同时保留 `companion.respond` 回流通道。
- 当前剩余接入点：用户主动向 companion run 发送消息后打开 human route 需要把 launch provenance 投到 execution context，再由 `human.ask` authority 判断。

## Validation Run

- `cargo fmt`
- `cargo check -p agentdash-application`
- `cargo test -p agentdash-application capability`
- `cargo test -p agentdash-application companion`
- `pnpm --filter app-web run typecheck`
