# CB04-C Session context usage projection 迁移

## Goal

将 context usage 分析从 contracts helper 移到 application session projection。

## Requirements

- `agentdash-contracts` 保留 `SessionContextUsageItemResponse` 等 response DTO。
- SPI `ContextFrame` 分析、usage 分类和 projection assembly 迁移到 application session projection。
- API/stream boundary 负责将 application usage read facts 映射到 response DTO。

## Acceptance Criteria

- [ ] contracts crate 不再分析 SPI `ContextFrame`。
- [ ] application session projection 输出 context usage read facts。
- [ ] session eventing / trace response 保持同等 context usage 信息。
- [ ] tests 覆盖 context usage 分类和 response mapping。

## Notes

- Good first-wave task; expected write set is narrow.
