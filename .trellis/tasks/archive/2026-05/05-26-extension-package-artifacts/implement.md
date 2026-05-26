# Implementation Plan

## Steps

- [ ] 定义 artifact domain model / repository。
- [ ] 新增 migration 和 Postgres repository。
- [ ] 新增 upload / download API route。
- [ ] 在 Project extension installation 中记录 artifact ref。
- [ ] 增加 archive manifest/digest validation。
- [ ] 为 `agentdash-local` 增加 artifact download/cache helper。
- [ ] 增加 tests 覆盖 artifact 生命周期。

## Validation

```powershell
cargo test -p agentdash-domain
cargo test -p agentdash-infrastructure
cargo test -p agentdash-api
cargo test -p agentdash-local
```

## Dependencies

依赖 `extension-runtime-contracts` 确定 manifest / bundle refs 字段。
