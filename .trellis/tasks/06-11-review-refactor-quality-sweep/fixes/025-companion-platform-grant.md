# Fix 025: companion platform grant

## 问题

`companion_request(target=platform, payload.type=capability_grant_request)` 原先会构造 `prompt` / `options` 后转入 `execute_human_request`。这会把平台 capability grant 申请表现成人类 companion request，但授权事实并未进入 `PermissionGrantService`、`PermissionGrant` 状态机或 capability runtime transition。

## 改动

- `target=platform` 的 `capability_grant_request` 不再调用 `execute_human_request`。
- 删除只服务于伪装 human request 的 grant prompt/options 构造 helper。
- 在 platform broker 尚未接入 `PermissionGrantService::request` 所需 policy inputs 与 live capability update handoff 前，明确返回缺少 platform permission grant broker 的执行错误。
- 将旧测试改为验证 missing broker / missing policy input 的失败语义。

## 涉及文件

- `crates/agentdash-application/src/companion/tools.rs`

## 验证

- `cargo fmt --package agentdash-application`：通过。
- `cargo test -p agentdash-application companion`：通过，39 passed，0 failed；输出保留既有 `session::construction` dead_code warnings。

## ARCH-010 关系

本批只关闭错误的 human request 降级路径，不实现完整 broker。ARCH-010 仍需统一 platform broker、`PermissionGrantService::request` policy input 装配、用户审批 handoff、AgentFrame capability effect application，以及 live runtime tool schema 更新。

## Commit

- hash: `709ac9a7`
