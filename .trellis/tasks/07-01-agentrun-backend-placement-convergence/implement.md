# 实施计划

## Context

当前任务处于 planning。实现前必须先完成 PRD review，并按 Trellis 进入 in_progress。

## Checklist

1. 合同与类型
   - 在 contract crate 增加 backend selection DTO。
   - 为 `CreateProjectAgentRunRequest` 与 `AgentRunComposerSubmitRequest` 增加 `backend_selection`。
   - 运行 contracts 生成并更新前端 generated 文件。

2. 数据库与 domain
   - 新增 migration：mailbox message planning facts、AgentRun sticky backend preference。
   - 更新 domain entity、repository trait、Postgres repository mapping。
   - 确保 command receipt digest 包含 backend selection。

3. AgentRun mailbox 链路
   - command structs 增加 backend selection/planning facts。
   - accept_user_message / accept_intake_message 持久化 planning facts。
   - scheduler consume 时从 message 读取 planning facts。
   - `AgentRunMessageDelivery` 携带 `LaunchPlanningInput`，替换固定 default。

4. Sticky preference
   - 在 AgentRun control-plane 层读取默认 backend preference。
   - 无 request selection 时生成默认 planning input。
   - explicit selection connector accepted 后更新 sticky preference。
   - failed launch 不更新 sticky preference。

5. Planner 授权与一致性
   - planner 或 placement policy port 读取 ProjectBackendAccess active grants。
   - explicit / workspace_binding / auto_idle 均限定在 active grants。
   - selected backend 与 VFS mount backend 不一致时，按 current workspace binding 重解析或失败。
   - lease 写入 workspace/root/selection mode/failure reason。

6. 前端选择与错误
   - 在 AgentRun workspace composer 附近提供 backend selector / 当前默认 backend 显示。
   - 将 selection 写入 draft start / composer submit payload。
   - immediate API error 进入 composer inline error 摘要，并通过全局通知/详情面展示完整错误。
   - mailbox consume failure 显著展示摘要，并提供完整错误查看入口。
   - queued message 异步失败通过外层通知显著提示，并按 message/error 去重。
   - 修正现有错误 UI 只 truncate 的问题：完整诊断不能只存在于截断文本。

7. 测试
   - 后端 contract tests / mailbox tests / planner placement tests / repository mapping tests。
   - 前端 payload tests / error display tests / selector state tests。
   - 运行验证命令。

## Validation Commands

```powershell
pnpm run contracts:check
cargo test -p agentdash-application-runtime-session backend_execution_placement
cargo test -p agentdash-application-agentrun agent_run::mailbox
cargo check --workspace
pnpm run frontend:check
```

可按实际修改范围补充更精确的 cargo test 过滤器。

## Risk Points

- 不要只在 API route 传 selection；queued mailbox 会丢语义。
- 不要把 sticky backend preference 放进 runtime session meta；它属于 AgentRun control-plane。
- 不要让 auto idle 看到全局在线 executor；必须先按 ProjectBackendAccess 缩候选集。
- 不要把 backend A 的 VFS root 发给 backend B；必须重解析或失败。
- 不要吞掉 scheduler consume-time 错误；用户必须在 workspace 里看到。
- 不要只依赖 composer inline banner 或 mailbox row；这些区域当前会截断文本，完整错误必须进入外层通知/详情面。

## Review Gate Before Start

- design.md 中 sticky preference 字段归属已确认。
- implement.jsonl / check.jsonl 已换成真实 spec/research 条目。
