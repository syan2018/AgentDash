# Contract Boundary 执行计划

## Phase 1: Audit

- [ ] 列出 `agentdash-application` 到 `agentdash-contracts` 的 imports。
- [ ] 列出 `agentdash-contracts` 中 domain/SPI/protocol `From` / conversion。
- [ ] 标注 application read model、API adapter、contract DTO owner。

## Phase 2: Follow-up Tasks

- [ ] 对允许保留的 projection conversion 写明原因。
- [ ] 对需要迁移的 incoming command conversion 创建实现任务。
- [ ] 对高风险 application direct DTO construction 创建迁移任务。

## Validation

```powershell
rg "agentdash_contracts" crates/agentdash-application
rg "impl From<|TryFrom<" crates/agentdash-contracts/src
pnpm run contracts:check
```

