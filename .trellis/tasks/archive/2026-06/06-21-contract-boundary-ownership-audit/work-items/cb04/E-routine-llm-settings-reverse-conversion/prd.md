# CB04-E Routine LLM Settings reverse conversion cleanup

## Goal

将 Routine、LLM provider、Settings 的 reverse DTO conversion 移到 route/application command mapper。

## Requirements

- contracts crate 保留 DTO definitions 和 outbound projection。
- Routine dispatch strategy、LLM provider credentials/protocol values、Settings scope reverse parsing 迁移到对应 route/application command mapper。
- 每个迁移点保持 request validation 与 command semantics 可测试。

## Acceptance Criteria

- [ ] contracts crate 不再拥有 Routine / LLM / Settings 的 audited reverse domain conversion。
- [ ] route/application command boundary 显式解析 incoming DTO。
- [ ] outbound projection 和 generated TypeScript 不漂移。
- [ ] focused tests 覆盖三个小迁移点。

## Notes

- Good first-wave task if split internally by file cluster.
