# 能力状态机统一执行计划

## Step 1 · 收窄规划

1. 记录本轮设计边界：只合并 `CapabilityDelta` 与 `SetDelta`。
2. 保留 trait merge 争议结论：`CapabilityDimensionModule` 与 `DimensionDelta` 正交，不在本轮合并。

## Step 2 · 类型合并

1. 在 `agentdash-spi/src/connector/capability_delta.rs` 为 `SetDelta` 增加 `compute(old, new)`。
2. 删除 `agentdash-spi/src/hooks/mod.rs` 中本地 `CapabilityDelta` 定义，改为使用/re-export `SetDelta`。
3. 将 application session 中 `CapabilityDelta` import 与字段类型替换为 `SetDelta`。
4. 保持 JSON 字段 shape 不变。

## Step 3 · 验证

1. `rg "struct CapabilityDelta|enum CapabilityDelta" crates/agentdash-spi/src/hooks`
2. `rg "CapabilityDelta" crates/agentdash-application/src/session crates/agentdash-spi/src/hooks`
3. `cargo check --workspace`
4. `cargo test -p agentdash-application --lib capability`
5. `cargo test -p agentdash-application --lib session::capability`

## Step 4 · 收尾

1. 更新 `progress-checklist.md` 的 capability-state-unify 证据。
2. 若 application 测试命令因既有 test-only persistence 债务失败，记录准确失败范围，不用窄命令伪装全量通过。
3. 提交实现与 checklist。
