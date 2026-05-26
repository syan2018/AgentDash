# Implementation Plan

## Steps

- [ ] 扩展 domain value objects 和 validators。
- [ ] 如需 schema repair，新增 migration。
- [ ] 新增独立 extension_runtime application projection 模块，并让 session construction 读取该 projection。
- [ ] 扩展 Project 级 API response DTO / contract generation。
- [ ] 新增前端 extension runtime types/service mapper。
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
