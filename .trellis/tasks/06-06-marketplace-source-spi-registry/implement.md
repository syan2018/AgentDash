# Implement · Marketplace Source SPI 与 Integration Registry

## 执行步骤

1. 新增 SPI 模块
   - 在 `crates/agentdash-spi/src/platform/marketplace_source.rs` 定义 provider trait 与 DTO。
   - 从 `crates/agentdash-spi/src/lib.rs` re-export 必要类型。
   - 保持依赖轻量；若需要时间字段，优先使用项目已用的 `chrono`。

2. 扩展 Integration API
   - 在 `crates/agentdash-integration-api/src/integration.rs` 引入 `MarketplaceSourceProvider`。
   - 在 `AgentDashIntegration` 上新增 `marketplace_source_providers()` 默认空实现。
   - 从 `crates/agentdash-integration-api/src/lib.rs` re-export marketplace source 类型。

3. 扩展宿主注册结果
   - 在 `crates/agentdash-api/src/integrations.rs` 的 `HostIntegrationRegistration` 增加 provider 列表。
   - 在 `collect_integration_registration` 中收集 provider。
   - 新增 `DuplicateMarketplaceSourceKey` / `InvalidMarketplaceSourceDescriptor` 等错误分支。
   - 校验 `source_key`、supported asset types 和重复 key。

4. 增加 first-party 示例 source
   - 在 `crates/agentdash-first-party-integrations/src/lib.rs` 新增一个空/示例 marketplace source provider。
   - 保证它声明 `skill_template` 和/或 `mcp_server_template`，返回空 page。

5. 测试
   - `collect_integration_registration` 能收集 marketplace source provider。
   - 重复 `source_key` 启动注册失败。
   - 不支持的 asset type 在收集阶段失败。
   - first-party integration 的示例 source descriptor 可读取。

## 验证命令

```powershell
cargo test -p agentdash-spi -p agentdash-integration-api -p agentdash-api -p agentdash-first-party-integrations marketplace
```

如测试过滤粒度不足，运行：

```powershell
cargo test -p agentdash-spi -p agentdash-integration-api -p agentdash-api -p agentdash-first-party-integrations
```

## 风险点

- `agentdash-spi` 与 `agentdash-integration-api` 形成循环依赖：Marketplace Source DTO 应依赖领域资产类型或轻量共享类型，Integration API 只 re-export。
- provider trait 过早绑定具体 HTTP catalog：本 child 只定义抽象，不加入 HTTP implementation。
- registry 只保存 provider 而不校验 descriptor：后续 API 会把错误暴露到请求期；本 child 应在启动收集期完成 fail-fast。

## 交付检查

- [ ] SPI 与 Integration API 编译通过。
- [ ] 宿主注册测试覆盖成功和失败路径。
- [ ] first-party 示例 source 不影响现有 integration seed / auth / connector 测试。
- [ ] 未修改 external marketplace API、Skill import 或前端。
