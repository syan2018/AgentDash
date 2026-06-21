# CB04-D Implementation Plan

## Steps

- [x] Introduce application read model types for capability catalog.
- [x] Change catalog service to return application read model.
- [x] Add adapter mapping in API route/module to contract DTO.
- [x] Update focused application/API tests.

## Validation

```powershell
cargo test -p agentdash-application capability
cargo test -p agentdash-api capability --lib
pnpm run contracts:check
```

## Completion Notes

- Application read model now lives in `agentdash_application::capability::tool_catalog`.
- API route `workflows::query_tool_catalog` maps the read model into `agentdash-contracts::workflow::CapabilityCatalogResponse`.
- Contract source and generated TypeScript were not changed.

## Validation Results

- `cargo test -p agentdash-api workflows --lib` passed.
- `cargo check -p agentdash-api` passed.
- `cargo test -p agentdash-application capability --lib` was blocked by an unrelated test compile error in `crates/agentdash-application/src/workspace_module/tools.rs`.

## Dispatch Notes

- Second-wave task unless no worker is editing capability/exposure modules.
