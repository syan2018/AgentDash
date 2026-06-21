# CB04-F Backend access command conversion owner review

## Goal

审计 backend access/status/mode reverse conversion，并将 command parsing owner 收口到 API adapter/application command boundary。

## Requirements

- 先审计 backend access/status/mode reverse conversions 是否实际服务 request command parsing。
- response projection 可以保留在 contracts，只要它是 narrow outbound domain -> DTO mapping。
- command/status parsing 若参与 request semantics，则迁移到 API adapter/application command builder。

## Acceptance Criteria

- [ ] 输出 backend access conversion owner review 结果。
- [ ] 必要 reverse command conversion 移出 contracts。
- [ ] response projection 保持明确方向和测试覆盖。
- [ ] 若无需代码迁移，任务以审计结论和 spec 更新完成。

## Notes

- Lower priority; can run as research/check worker before implementation.
