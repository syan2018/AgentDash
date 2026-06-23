# Work Item Index

## DAG

See `../parallel-dag.md`.

## Items

| Item | Status | Summary |
| --- | --- | --- |
| `WI-00-baseline-import-contract-inventory.md` | done | Establish baseline imports, contracts and forbidden dependency list. |
| `WI-01-agentrun-target-adoption-port.md` | done | Move AgentFrame runtime target/adoption ownership to AgentRun. |
| `WI-02-current-resource-surface-facades.md` | done | Stabilize current/resource surface DTOs and facades. |
| `WI-03-runtime-session-public-facade.md` | pending | Tighten `session` public facade to RuntimeSession substrate. |
| `WI-04-runtime-gateway-mcp-boundary.md` | pending | Harden RuntimeGateway MCP access against SessionHub/current frame fallback. |
| `WI-05-api-current-surface-consumers.md` | pending | Migrate API, VFS and Terminal current-surface consumers. |
| `WI-06-surface-update-unification.md` | pending | Unify business surface updates behind AgentRun typed update facade. |
| `WI-07-launch-commit-ownership.md` | pending | Split RuntimeSession delivery commit from AgentRun/Lifecycle writes. |
| `WI-08-presentation-read-model-cleanup.md` | pending | Move presentation/current-frame read models behind application query facades. |
| `WI-09-public-visibility-import-cleanup.md` | pending | Remove broad public exports and forbidden imports after migrations. |
| `WI-10-final-review-gate.md` | pending | Run final evidence, test and documentation gate. |
| `WI-11-canvas-extension-session-project-binding.md` | pending | Add explicit Canvas/Extension Project/session binding guards before Gateway/provider invocation. |

## Tracking Rules

Every item doc must be updated during implementation with:

- assigned worker
- start/end status
- files changed
- tests run
- blockers or follow-up notes
- handoff summary
