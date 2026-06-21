# CB04-A MCP preset incoming conversion 迁移

## Goal

将 MCP preset request DTO 到 domain/application command 的 incoming conversion 移出 contracts。

## Requirements

- `agentdash-contracts` 保留 MCP preset request/response DTO、serde wire shape、TypeScript generation 和 outbound response projection。
- DTO -> domain transport/runtime binding/route policy 的 incoming conversion 迁移到 API adapter 或 application command builder。
- create/update/probe 的 patch 语义、required/optional runtime binding 语义和 route policy 语义由 command boundary 持有。
- 不引入兼容路径；预研期直接收敛到目标 owner。

## Acceptance Criteria

- [ ] contracts crate 不再拥有 MCP preset incoming command/domain conversion。
- [ ] route/application command path 明确执行 request DTO -> domain value mapping。
- [ ] outbound MCP preset response projection 仍保持可生成、可序列化。
- [ ] 现有 MCP preset create/update/probe 行为有 focused tests 覆盖。

## Notes

- Parent owner map: `.trellis/tasks/06-21-contract-boundary-ownership-audit/owner-map.md`
