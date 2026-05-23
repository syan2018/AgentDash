# Persistence Ports 分层下沉 Implement

## Order

1. 生成依赖清单：
   ```powershell
   rg -n "agentdash_application::" crates/agentdash-infrastructure/src
   ```
2. 按类型分类：
   - trait port；
   - DTO/record；
   - error；
   - orchestration helper。
3. 选择一个最小迁移切片。
4. 将 trait/record 移到目标 contract 层。
5. 调整 application 与 infrastructure import。
6. 运行 package check。
7. 更新 spec。

## Validation

```powershell
cargo check -p agentdash-infrastructure -p agentdash-application -p agentdash-api
rg -n "agentdash_application::" crates/agentdash-infrastructure/src
```

## Rollback Points

- 每次只迁移一个 port cluster。
- 若新 crate 牵涉 Cargo workspace churn，优先放入现有 `agentdash-spi` 或 `agentdash-domain` 的窄模块，待边界稳定后再拆 crate。
