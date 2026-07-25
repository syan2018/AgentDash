# S5 Combined Activation Freeze

## Status

Wave 4 is frozen and independently reviewed. This directory is the executable handoff from the
four domain activation worktrees into the single S5 hard-cut staging worktree.

It is not a production checkpoint. The production path on the stable branch remains the S4 path
until the complete S5 staging tip passes both architecture and behavior review.

## Frozen inputs

| Component | Final tip | Review result | S5 role |
| --- | --- | --- | --- |
| Platform Runtime | `30d9a55597e36fc5af0591c420346c3217c1dbae` | `component_ready: pass` | final Runtime/Host/Surface/Wire contracts and owner cleanup |
| Dash / Native | `6c38dd3de7527859f21e21b28a6b7cb37c7e0f5c` | `component_ready: pass` | Agent/Core physical activation, Native Complete Agent and atomic store |
| External Agents | `ffaf54a749659923e28599fe075616d34c292b43` | `component_ready: pass` | Codex/Remote Complete Agent activation and legacy adapter cleanup |
| Product / Protocol | `67d9eef5f078dcb10077bbdb2eab1a05d2a33674` | `shared_foundation_input: pass` | Product graph/context contracts and exact caller inventory |

All inputs are based on `fc26d3ffb951461d8e9214b6b4639b88c18d533d`, have clean worktrees,
and retain the base `Cargo.lock`.

Product caller activation is intentionally sequenced inside S5. Its domain component is complete,
but the real Application/API/UI switch requires the final PostgreSQL bindings, AppState ports and
canonical Managed Runtime TypeScript output owned by W8. No temporary DTO, facade or fallback is
allowed.

## Integration sequence

1. Apply the four owner commit ranges in the order recorded in `manifest.json`.
2. Resolve the sole same-file overlap,
   `crates/agentdash-application-agentrun/Cargo.toml`, as the union of:
   - Dash removal of the direct `agentdash-agent` dependency;
   - Product's dev-only canonical parity dependency on `agentdash-agent-service-api`.
3. W8 implements the frozen PostgreSQL repositories and the single forward migration.
4. W8 creates the final AppState repository bindings and canonical Runtime/Service/Wire generated
   outputs required by the Product manifest.
5. Main temporarily transfers the exact caller files listed by the Product manifest to the original
   Product/Protocol owner, which performs the real source switch in the S5 staging worktree.
6. Product checker rechecks the caller switch and combined Product gates.
7. W8 completes production composition, zero-consumer crate/schema deletion and the only
   `Cargo.lock` generation.
8. Architecture and behavior checkers independently review the complete staging tip.

No intermediate staging commit is a stable checkpoint.

## Ownership

- Platform Runtime continues to own Runtime Contract, Managed Runtime, Host, Surface/Callback and
  Runtime Wire business semantics.
- Dash/Native continues to own Dash Agent, AgentCore, Native service and Native legacy removal.
- External Agents continues to own Codex/Remote behavior and adapter legacy removal.
- Product/Protocol continues to own AgentRun, Companion, API/App Server/UI callers and feed
  semantics.
- W8 owns the formal migration, PostgreSQL adapters, workspace/lockfile, production composition,
  canonical generated artifacts and zero-consumer physical deletion.

Any failure in domain behavior returns to its original owner. W8 must not rebuild a missing domain
contract or add a compatibility path.

## Verification

Run before dispatching the hard-cut integrator:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File `
  .trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/activation/s5-combined/verify-inputs.ps1
```

The script verifies exact tips, clean worktrees, common base, lock ownership, component artifacts
and the single expected overlap.
