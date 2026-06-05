# Implement · Enterprise Marketplace API 与 Contracts

## 执行步骤

1. Contracts
   - 新增 external marketplace DTO。
   - 更新 `crates/agentdash-contracts/src/generate_ts.rs`。
   - 生成/更新 `packages/app-web/src/generated/...`。

2. Application import helper
   - 在 shared library application 层新增 external marketplace import use case。
   - 输入 provider fetched payload，输出 `LibraryAsset`。
   - 使用 canonical payload digest 与 `LibraryAssetPayload` validator。
   - import mode 首期只支持 `upsert_library_asset`。

3. API routes
   - 新增 `crates/agentdash-api/src/routes/marketplace.rs` 或等价外部 marketplace route。
   - 接入 app router。
   - 从 `state.services.marketplace_source_providers` 查找 providers。
   - 实现 sources/list/detail/import/refresh。

4. Tests
   - API/route 或 application tests 用 fake provider 覆盖成功和错误路径。
   - Contract TS 生成后确认前端类型导出。

## 主要文件

- `crates/agentdash-contracts/src/...`
- `crates/agentdash-contracts/src/generate_ts.rs`
- `crates/agentdash-api/src/routes/...`
- `crates/agentdash-api/src/app_state.rs`
- `crates/agentdash-application/src/shared_library/...`
- `packages/app-web/src/generated/...`

## 验证命令

```powershell
cargo test -p agentdash-contracts -p agentdash-application -p agentdash-api marketplace
pnpm --filter @agentdash/app-web typecheck
```

如 TS 生成命令已有项目脚本，使用现有 contract generation 命令。

## 风险点

- 多 source 聚合分页 cursor 语义不稳定：首期 cursor 必须绑定单 source。
- import route 直接接受前端 raw payload：必须由 provider fetch payload，前端只提交 source identity。
- 远端 digest 与平台 payload digest 混淆：平台 digest 仍由 canonical payload 计算。
- API child 过度实现 Skill/MCP materializer：本 child 只消费 provider 返回的 fetched payload。

## 交付检查

- [ ] Routes 可从 first-party empty source 返回 source list。
- [ ] Contract TS 文件生成。
- [ ] import use case 有 payload validator 测试。
- [ ] refresh 不写 Project 资源。
- [ ] 未修改 Skill URL Import、MCP install 参数和前端 Marketplace UI。
