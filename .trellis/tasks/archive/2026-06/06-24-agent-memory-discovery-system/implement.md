# Implementation Plan Draft

> Draft only. Start implementation after PRD/design review.

## Phase 0 - Recovered Context

- [x] ProjectAgent launch path inspected.
  - `composer_project_agent.rs` passes `project_agent.id` into `OwnerScope::Project`.
  - `owner_bootstrap.rs` builds VFS through `VfsService::build_vfs` and applies grants, but does not call `append_agent_knowledge_mounts`.
  - `mount_project.rs` contains the reusable `append_agent_knowledge_mounts` helper and `build_project_agent_knowledge_vfs` preview surface.
  - Result: runtime ProjectAgent sessions currently miss the Agent memory mount.
- [x] Skill discovery wiring inspected.
  - SPI: `crates/agentdash-spi/src/platform/skill_discovery.rs`
  - Integration contract: `crates/agentdash-integration-api/src/integration.rs`
  - Registry: `crates/agentdash-api/src/integrations.rs`
  - Runtime projection: `crates/agentdash-application/src/agent_run/runtime_capability_projection.rs`
  - First-party integrations: `crates/agentdash-first-party-integrations/src/lib.rs`
  - Result: Memory discovery should mirror this registration and conflict model.
- [x] Runtime context injection inspected.
  - Production owner bootstrap currently calls `derive_runtime_skill_baseline`, not full `derive_runtime_capability_projection`.
  - `FrameLaunchIntent` carries discovered guidelines, and `TurnPreparer` builds `system_guidelines` frames.
  - Assignment context only renders whitelisted semantic slots; memory needs a dedicated frame or an explicit slot update.
  - Result: implement memory projection in production frame construction, then pass inventory into launch/turn preparation.
- [x] Existing `MEMORY.md` guideline behavior inspected.
  - `BUILTIN_GUIDELINE_RULES` scans both `AGENTS.md` and `MEMORY.md`.
  - Result: move `MEMORY.md` semantics to memory context during this task.

## Parallel Dispatch Strategy

The implementation should be split by dependency boundary and touched files. The optimal order is:

```text
Wave 1A: VFS mount rename + ProjectAgent runtime mount
Wave 1B: Memory discovery SPI + Host Integration registry
Wave 1C: memory-manager skill draft
        ↓
Wave 2: first-party provider + runtime memory projection + memory context frame
        ↓
Wave 3: frontend polish, stale string cleanup, full check pass
```

### Wave 1A - VFS Runtime Subtask

Scope:

- Rename `agent-knowledge` mount id to `agent`.
- Add the ProjectAgent Agent mount to active runtime VFS when `knowledge_enabled=true`.
- Keep `ProjectAgentKnowledge` surface source name stable and point its visible mount to `agent`.

Primary files:

- `crates/agentdash-application/src/vfs/mount_project.rs`
- `crates/agentdash-application/src/vfs/mount.rs`
- `crates/agentdash-application/src/agent_run/frame/construction/composer_project_agent.rs`
- `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs`
- `packages/app-web/src/features/project/agent-preset-editor/knowledge-section.tsx`

Independent validation:

- Runtime construction test: enabled Agent gets `agent` mount.
- Runtime construction test: disabled Agent omits `agent` mount.
- Search: `rg -n "agent-knowledge" crates packages tests`.

Dispatch priority: first. This unblocks all memory source tests because `agent://` must exist in active VFS.

### Wave 1B - SPI / Integration Subtask

Scope:

- Add Memory discovery types to SPI.
- Re-export them through `agentdash-spi` and `agentdash-integration-api`.
- Add `AgentDashIntegration::memory_discovery_providers()`.
- Extend host integration collection with provider-key validation.

Primary files:

- `crates/agentdash-spi/src/platform/memory_discovery.rs`
- `crates/agentdash-spi/src/platform/mod.rs`
- `crates/agentdash-spi/src/lib.rs`
- `crates/agentdash-integration-api/src/integration.rs`
- `crates/agentdash-integration-api/src/lib.rs`
- `crates/agentdash-api/src/integrations.rs`
- `crates/agentdash-api/src/app_state.rs`
- session/bootstrap dependency structs that currently carry skill discovery providers

Independent validation:

- SPI unit tests for bounded defaults and controlled URI validation.
- Integration collection tests for provider collection, empty key, and duplicate key.
- `cargo test -p agentdash-spi`
- `cargo test -p agentdash-integration-api`
- targeted `cargo test -p agentdash-api integrations`

Dispatch priority: parallel with Wave 1A. It does not need runtime mount injection until first-party provider tests are added.

### Wave 1C - Skill Content Subtask

Scope:

- Draft bundled `memory-manager` skill using `agent://` as the default home.
- Encode file layout, write gate, stale claim verification, and secret scan guidance.
- Keep examples on normal VFS tools.

Primary evidence:

- `research/claude-code-memory-prompts.md`

Independent validation:

- Skill metadata/frontmatter parse test.
- Existing bundled skill discovery/packaging test pattern.

Dispatch priority: parallel with Wave 1A and 1B. It only depends on the agreed `agent://` convention and can merge later if packaging paths overlap with runtime skill projection.

### Wave 2 - Runtime Memory Projection Subtask

Start condition:

- Wave 1A and 1B merged or available in the same worktree.

Scope:

- Add first-party ProjectAgent memory discovery provider.
- Build application projection helper from active VFS + memory providers.
- Thread memory inventory through frame construction, launch plan, and turn preparation.
- Add `memory_context_frame`.
- Move `MEMORY.md` from guideline injection into memory context.

Primary files:

- `crates/agentdash-first-party-integrations/src/lib.rs`
- `crates/agentdash-application/src/agent_run/runtime_capability_projection.rs`
- `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs`
- `crates/agentdash-application/src/agent_run/frame/construction/assembly.rs`
- `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs`
- `crates/agentdash-application/src/session/launch/plan.rs`
- `crates/agentdash-application/src/session/launch/preparation.rs`
- `crates/agentdash-application/src/context/mount_file_discovery.rs`
- new `crates/agentdash-application/src/session/memory_context_frame.rs`

Independent validation:

- Missing `agent://MEMORY.md` still yields source inventory with `index_status=missing`.
- Present bounded `MEMORY.md` appears in memory context.
- Oversized `MEMORY.md` yields diagnostic/status without injecting body.
- Topic file body is not injected by default.
- `MEMORY.md` no longer appears in `system_guidelines`.

Dispatch priority: after Wave 1 because it touches both runtime VFS facts and SPI contracts.

### Wave 3 - Integration Check Subtask

Scope:

- Run stale-string, contract, backend, and frontend verification.
- Check capability equality between memory source and mount.
- Check context frame visibility and audit behavior.
- Check ProjectAgent editor still browses Agent memory through `ProjectAgentKnowledge` source.

Validation:

- `cargo test -p agentdash-spi`
- `cargo test -p agentdash-integration-api`
- `cargo test -p agentdash-first-party-integrations`
- `cargo test -p agentdash-application`
- `cargo test -p agentdash-api`
- relevant frontend tests for ProjectAgent editor / VFS browser
- `rg -n "agent-knowledge" crates packages tests .trellis/spec`

Dispatch priority: final check subagent after implementation waves merge. A focused check pass is more useful here than per-wave broad testing because Wave 2 is the first point where VFS, provider registry, launch handoff, and context frame all meet.

## Phase 1 - Rename ProjectAgent Mount

- [ ] Change Agent knowledge mount id from `agent-knowledge` to `agent`.
  - `crates/agentdash-application/src/vfs/mount_project.rs`
- [ ] Keep inline storage container id as `knowledge`.
  - Reason: storage identity already means ProjectAgent knowledge container; URI/mount naming is the part being corrected.
- [ ] Update mount purpose detection for `agent`.
  - `crates/agentdash-application/src/vfs/mount.rs`
- [ ] Update ProjectAgent knowledge browser to use `agent`.
  - `packages/app-web/src/features/project/agent-preset-editor/knowledge-section.tsx`
- [ ] Update tests and snapshots that assert the old mount id.
  - Search: `rg -n "agent-knowledge" crates packages tests .trellis/spec`

## Phase 2 - Add Agent Mount To Runtime VFS

- [ ] In ProjectAgent owner bootstrap, load the current `ProjectAgent` or pass it through the bootstrap spec so `knowledge_enabled` is available where VFS is built.
  - Primary files:
    - `crates/agentdash-application/src/agent_run/frame/construction/composer_project_agent.rs`
    - `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs`
- [ ] Append the `agent` mount when `knowledge_enabled=true`.
  - Reuse `append_agent_knowledge_mounts`.
  - Apply after base VFS construction and before skill/memory discovery consumes active VFS.
- [ ] Keep `apply_agent_vfs_access_grants` scoped to Project VFS mounts.
  - Existing helper already filters with `is_project_vfs_mount`.
- [ ] Add runtime construction tests:
  - enabled ProjectAgent gets `agent` mount with read/write/list/search;
  - disabled ProjectAgent omits `agent`;
  - `CapabilityState.vfs.active` equals the final VFS.

## Phase 3 - Add Memory Discovery SPI

- [ ] Add `crates/agentdash-spi/src/platform/memory_discovery.rs` and export it through SPI/lib re-exports.
- [ ] Define:
  - `MemoryDiscoveryContext`
  - `MemoryDiscoveryMount`
  - `MemoryDiscoveryVfsRule`
  - `MemoryDiscoveryVfsFile`
  - `DiscoveredMemorySource`
  - `MemoryDiscoveryCluster`
  - `MemoryDiscoveryDiagnostic`
  - `MemoryDiscoveryOutput`
  - `MemoryDiscoveryProvider`
- [ ] `MemoryDiscoveryMount` contains only controlled runtime mount facts: id, provider, display name, owner/purpose summary, capabilities.
- [ ] `DiscoveredMemorySource.capabilities` is copied from the resolved mount summary.
- [ ] Add duplicate source normalization and diagnostics using provider key + source key.
- [ ] Validate VFS-first provider output URI shapes with the same controlled URI policy used for dynamic skill paths.

## Phase 4 - Wire Through Host Integration

- [ ] Extend `AgentDashIntegration` with `memory_discovery_providers()`.
  - `crates/agentdash-integration-api/src/integration.rs`
  - `crates/agentdash-integration-api/src/lib.rs`
- [ ] Extend host collection and duplicate provider-key validation.
  - `crates/agentdash-api/src/integrations.rs`
  - `crates/agentdash-api/src/app_state.rs`
  - bootstrap/session construction dependency structs that currently carry skill providers
- [ ] Add a first-party provider for the default ProjectAgent Agent mount.
  - It recognizes `mount_id == "agent"` and returns `agent://`.
  - It declares VFS rules for `MEMORY.md` so bounded index content can be read when present.
  - It still returns a source when the index file is missing.

## Phase 5 - Runtime Projection And Context Injection

- [ ] Add application helper parallel to skill baseline:
  - derive memory inventory from active VFS, memory discovery providers, and bounded VFS reads.
- [ ] Call this helper from production ProjectAgent owner bootstrap after final active VFS exists.
- [ ] Carry discovered memory inventory through launch handoff.
  - Likely fields: `FrameAssemblyLaunchExtras`, `FrameLaunchIntent`, `LaunchPlan`.
- [ ] Add `memory_context_frame` and include it in `TurnPreparer` alongside `system_guidelines`.
- [ ] Move `MEMORY.md` out of `BUILTIN_GUIDELINE_RULES` so memory index is not injected twice.
- [ ] Context frame content:
  - policy;
  - source inventory;
  - default source `agent://`;
  - index pointer `agent://MEMORY.md`;
  - bounded index excerpt when available.
- [ ] Add prompt/context tests for missing index, present index, oversized index, and no topic body injection.

## Phase 6 - Bundled `memory-manager` Skill

- [ ] Add bundled skill files in the same packaging path as existing system/bundled skills.
- [ ] Skill instructions cover:
  - short `MEMORY.md` index;
  - topic files with frontmatter;
  - high-signal write gate;
  - updating existing topics before creating new ones;
  - secret scan before shared memory writes;
  - stale claim verification before use.
- [ ] Skill uses normal VFS tools in examples and references `agent://` as the default home.
- [ ] Add discovery/packaging tests consistent with existing bundled skill tests.

## Phase 7 - Final Alignment Checks

- [ ] `rg -n "agent-knowledge" crates packages tests .trellis/spec` is clean or only appears in archived task discussion.
- [ ] Memory provider registration follows Host Integration fail-fast duplicate key behavior.
- [ ] Memory source capabilities match resolved mount capabilities in tests.
- [ ] Memory context is present for `knowledge_enabled=true` and absent when the mount is absent.
- [ ] `MEMORY.md` is no longer injected through `system_guidelines`.
- [ ] No implementation path reads raw host paths for memory discovery.

## Validation Commands

- [ ] `cargo test -p agentdash-spi`
- [ ] `cargo test -p agentdash-integration-api`
- [ ] `cargo test -p agentdash-first-party-integrations`
- [ ] `cargo test -p agentdash-application`
- [ ] `cargo test -p agentdash-api`
- [ ] Relevant frontend tests for ProjectAgent editor / VFS browser.
- [ ] Contract generation if DTO exports change.
