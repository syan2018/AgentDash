# Auth 身份完整接入修复 Implement Plan

## Checklist

- [x] 读取并确认相关规格：企业身份/Project sharing、后端 API 边界、Project backend routing、VFS access、Session startup、runtime gateway、relay protocol。
- [x] 给 auth middleware 接入 identity projection 与 provider-level authorize。
- [x] 为 middleware/provider 行为补测试：authorize deny 返回 403、provider error 保持 service unavailable 映射。
- [x] 收口 MCP transport：请求进入 MCP service 前必须有 identity；Project/Story/Task/Workflow 目标必须通过 Project permission。
- [x] 给 MCP relay/story/task/workflow 的读写 tool 增加权限判定，并覆盖 Project view/edit 边界测试。
- [x] 修复 terminal list/input/resize/kill：从 terminal id/session id 回溯 Project，并对 current user 调 `ensure_session_permission`。
- [x] 修复 Backend 路由：所有全局 backend handler 接入 `CurrentUser`，按 owner/scope/admin 收口；add/ensure/browse runtime actor 带 current user。
- [x] 修复 VFS surface、`/api/vfs/*`、file-picker helper：VFS read/list/stat/mutation 传入当前身份。
- [x] 检查 Session/Task launch identity 链路，确认既有 `TaskExecutionInput.requested_by` 与 Session construction identity 链路未断开。
- [x] 同步前端 API/types 调用点，确认本次无 DTO/contract 变更。
- [x] 运行 Rust check/test，并记录结果。
- [x] 将 Project permission enum/判定方法沉到 domain 层，让 API、application 与 MCP 共用 `ProjectAuthorizationService`。
- [x] 将 Backend owner/scope/admin/personal 判定沉到 application 层 `BackendAuthorizationService`，删除 route 层重复 helper。
- [x] 更新 API/MCP 调用点和错误映射，确保 handler 只消费统一授权服务。
- [x] 补 domain/application 授权服务测试并重新运行 Rust 验证。

## Suggested Order

1. Auth middleware and identity directory first，因为后续所有 route 测试都依赖稳定 `CurrentUser`。
2. Terminal and Backend next，因为它们是直接操作 runtime 的高风险 surface。
3. VFS identity propagation next，因为函数签名已经支持 identity，主要是 API 层补传。
4. MCP last，但必须进入本任务验收；它需要在 transport 与 tool dispatch 之间找到最小侵入的 request identity carrier。
5. Frontend/type generation cleanup after backend contracts settle。

## Validation Commands

```powershell
cargo fmt --all --check
cargo test -p agentdash-api
cargo test -p agentdash-application
cargo test -p agentdash-mcp
cargo test -p agentdash-infrastructure
pnpm --filter app-web typecheck
pnpm --filter app-web test
```

根据实际改动面补充更窄或更全的 workspace 命令。

## Risk Points

- MCP rmcp service 是否能自然访问 Axum request extensions；如果不能，需要一个明确的 request context carrier。
- Backend 全局列表在 personal 与 enterprise 下的权限语义不同，需要与 Settings system scope 的 admin 规则保持一致。
- middleware 每次同步身份目录会增加数据库写入，需保证 upsert 幂等并覆盖失败语义。
- Terminal cache 必须能可靠从 terminal id 找到 session id，否则需要先补 cache state。
- VFS API 与 runtime tools 已有两条入口，改动时要保持 session runtime tool 身份链路不回退。

## Follow-Up Checks Before Start

- 无剩余阻塞决策；规划可进入 `task.py start`。
