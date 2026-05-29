# 能力状态机统一设计

## 目标边界

本轮只处理 wave2 重审确认的安全闭环：`hooks::CapabilityDelta` 与 `connector::capability_delta::SetDelta` 是同一 crate 内完全同构的 `{ added, removed }` set diff 表达，应统一为 `SetDelta`。

不合并 `CapabilityDimensionModule` 与 `DimensionDelta`。二者分别服务 effect replay 与 render/projection，输入、输出和维度覆盖不一致；强行合并会把 effect mutation、UI section render、文本 render 混进一个 trait，降低职责清晰度。

不在本任务处理 `surface.vfs` / `context_projection.vfs` 单存储派生。该项归入 `session-assembly-converge` 复核线，避免两个 child 同时改 session launch 与 query route 消费面。

## 依赖方向

- `agentdash-spi::connector::capability_delta::SetDelta` 继续作为纯数据 + 纯函数 delta 模型。
- `agentdash-spi::hooks` 直接使用并 re-export `SetDelta`，删除本地 `CapabilityDelta` 类型。
- `agentdash-application::session` 消费 `SetDelta`，语义名由字段名承担，例如 `key_delta: SetDelta`、`capability_delta: Option<SetDelta>`。

这样 hook runtime、session transition、dimension render 共用同一 set delta 结构，同时不把 application 状态机逻辑上移到 SPI。

## 行为保持

`CapabilityDelta::compute(old, new)` 的行为迁到 `SetDelta::compute(old, new)`；`is_empty()` 保持不变。序列化字段仍是 `added` / `removed` 且 `snake_case`，因此 runtime notification payload 不改变。

## 验收

- `rg "struct CapabilityDelta|enum CapabilityDelta" crates/agentdash-spi/src/hooks` 无命中。
- `rg "CapabilityDelta" crates/agentdash-application/src/session crates/agentdash-spi/src/hooks` 无命中。
- `SetDelta::compute` 覆盖旧能力 key diff 调用点。
- `cargo check --workspace` 通过。
- capability/session 相关 application 测试通过。
