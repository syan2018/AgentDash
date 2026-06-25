# Work Item 08: Public Visibility Cleanup

## Objective

收缩 application root、session、agent_run、lifecycle、vfs 的 public facade，让 public exports 匹配未来 crate API。

## Owns

- `crates/agentdash-application/src/lib.rs`
- `crates/agentdash-application/src/session/mod.rs`
- `crates/agentdash-application/src/agent_run/mod.rs`
- `crates/agentdash-application/src/agent_run/frame/mod.rs`
- `crates/agentdash-application/src/lifecycle/mod.rs`
- `crates/agentdash-application/src/vfs/mod.rs`
- `crates/agentdash-application/src/runtime_tools/mod.rs`

## Implementation Strategy

1. Run facade count baseline with `rg`.
2. Change consumers to use intended facade/ports before lowering visibility.
3. Make builders, surface ext helpers, owner provider internals and session hub internals crate-private where possible.
4. Keep explicit public use-case services and DTOs required by API/local/MCP.
5. Use compile errors to find route/helper imports that still rely on internals.

## Completion Gates

```powershell
cargo check -p agentdash-application
cargo check -p agentdash-api -p agentdash-local -p agentdash-mcp
rg -n "^(pub mod|pub\\(crate\\) mod|mod |pub use|pub\\(crate\\) use)" crates/agentdash-application/src/lib.rs crates/agentdash-application/src/session/mod.rs crates/agentdash-application/src/agent_run/mod.rs crates/agentdash-application/src/lifecycle/mod.rs crates/agentdash-application/src/vfs/mod.rs
```

## Handoff

Report before/after facade counts and any public exports kept because API/local/MCP still consume them.
