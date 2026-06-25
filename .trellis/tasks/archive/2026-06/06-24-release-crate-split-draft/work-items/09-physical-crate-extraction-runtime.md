# Work Item 09: Physical Crate Extraction Runtime

## Objective

创建并抽取 `agentdash-application-runtime-gateway` 与 `agentdash-application-runtime-session`，让 runtime invocation 和 RuntimeSession substrate 的 implementation boundary 由 Cargo 强制。

## Owns

- workspace `Cargo.toml`
- `crates/agentdash-application-runtime-gateway/**`
- `crates/agentdash-application-runtime-session/**`
- moved RuntimeGateway / RuntimeSession module paths
- API/local/MCP composition root dependency updates

## Implementation Strategy

1. Add new crates to workspace and workspace dependencies.
2. Move RuntimeGateway modules first after setup/MCP deps are ports.
3. Move RuntimeSession substrate after launch/adoption/mailbox deps are ports.
4. Keep umbrella `agentdash-application` re-export only where it serves temporary composition; remove once consumers use direct crates.
5. Run `cargo metadata` after each crate add/move.

## Completion Gates

```powershell
cargo metadata --no-deps --format-version 1
cargo check -p agentdash-application-runtime-gateway
cargo check -p agentdash-application-runtime-session
cargo check -p agentdash-api -p agentdash-local -p agentdash-mcp
```

## Handoff

Report crate manifests, moved module list, umbrella re-exports, and remaining compile blockers grouped by downstream crate.
