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

## Progress

- 依赖清单显示 `agentdash-infrastructure` 只通过 session persistence 类型和 shared library digest 依赖 `agentdash-application`。
- 已将 `SessionPersistence`、session event record、terminal effect outbox record、runtime command record 和相关 payload contract 下沉到 `agentdash-spi::session_persistence`。
- 已将 shared library payload digest 下沉到 `agentdash-domain::shared_library::seed_digest`，让 repository 归一化逻辑不依赖 application service 模块。
- `agentdash-infrastructure` 的 Cargo 依赖已从 `agentdash-application` 切换为 `agentdash-spi`，源码中不再出现 `agentdash_application::` 引用。
- application 保留 `SessionStoreSet` adapter 和运行时编排，并 re-export 迁移后的 contract，避免外部调用面扩散。
