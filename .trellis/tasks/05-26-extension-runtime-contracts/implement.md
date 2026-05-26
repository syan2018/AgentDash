# Implementation Plan

## Steps

- [ ] 扩展 domain value objects 和 validators。
- [ ] 如需 schema repair，新增 migration。
- [ ] 扩展 session construction projection structs。
- [ ] 扩展 API response DTO / contract generation。
- [ ] 扩展前端 session context types 和 mapper。
- [ ] 增加 Rust tests：payload validation、projection flatten、conflict detection。
- [ ] 增加前端 tests：mapper 解析 extension runtime。

## Validation

```powershell
pnpm run contracts:check
cargo test -p agentdash-domain
cargo test -p agentdash-api session_use_cases::construction
pnpm run frontend:check
pnpm run frontend:test
```

## Dependencies

无实现依赖，是后续 package artifact、RuntimeGateway、WorkspacePanel 子任务的前置契约。
