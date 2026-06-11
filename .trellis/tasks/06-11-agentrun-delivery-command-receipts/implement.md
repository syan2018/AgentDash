# 实施计划

1. 定义 delivery command receipt domain type 和 repository trait。
2. 新增 forward migration 和 Postgres repository。
3. 更新 test-support memory persistence。
4. 在 ProjectAgentSessionStartService 入口创建/查询 start receipt。
5. 在 AgentRunMessageService 入口创建/查询 message receipt。
6. 将 receipt accepted 写入 launch accepted commit 或 command dispatch accepted 点。
7. 将 terminal failure 写入 receipt。
8. 增加 focused tests：
   - Project Agent start duplicate。
   - AgentRun message duplicate。
   - digest mismatch。
   - terminal failure retry。

## Validation

- `pnpm run migration:guard`
- `cargo test -p agentdash-application command_receipt`
- `cargo test -p agentdash-infrastructure command_receipt`
