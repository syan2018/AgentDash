# 零散启发备忘

本文件收纳 subagent 研究中暂未进入 `subagent-feature-synthesis.md` 主线优先级、但未来可能启发设计的观察。它不是任务清单，也不代表近期一定要实现。

## Runtime / Daemon

### Empty-claim cache 与 wakeup 思路

multica daemon poll task 时会结合 empty-claim cache 与 wakeup，避免大量 runtime 空轮询。AgentDash 当前是 cloud push relay，不应改成 poll claim，但调度层未来可学习“空状态缓存 + wakeup invalidation”的思路，用于减少对不可用 backend 的无效派发。

参考：`references/multica/server/internal/service/task.go`、`references/multica/server/internal/daemon/wakeup.go`。

### Pending relay request 的错误分类

AgentDash relay 已有 pending request，但用户可见错误仍可进一步区分 backend offline、command timeout、transport close、session terminal、local handler panic 等。multica 的 task failure reason 提醒我们：失败原因不是只给日志看的，它会影响重试、通知、UI 文案和统计。

参考：`crates/agentdash-api/src/relay/registry.rs`、`references/multica/server/internal/service/task.go`。

### Runtime timezone / device metadata

multica runtime 记录 timezone、device metadata、visibility 等信息。AgentDash 初期未必需要完整复制，但 desktop/local backend 产品化后，timezone 可影响 daily rollup，device metadata 可帮助用户区分多台机器上的 local backend。

参考：`references/multica/server/pkg/db/queries/runtime.sql`。

## Workdir / VFS

### Artifact-only cleanup

multica GC 不只是删除 completed/cancelled workdir，也有 artifact-only cleanup 的概念。AgentDash VFS materialization 未来可区分“可重建缓存”“执行产物”“用户应保留 artifact”，避免清理策略过粗。

参考：`references/multica/server/internal/daemon/gc.go`、`crates/agentdash-local/src/materialization.rs`。

### Provider 文件布局不等于 Prompt 拼接

AGENTS/CLAUDE/GEMINI/CODEX_HOME 等 provider-native 文件不只是减少 prompt token，也会影响 provider 对规则、skills、上下文的优先级理解。AgentDash 若做 Skill materialization，应把“生成了哪些文件、来自哪些 Skill/VFS source、用于哪个 session”记录到 manifest 或 session event。

参考：`references/multica/server/internal/daemon/execenv/execenv.go`。

## Frontend / Desktop

### Desktop settings slot 注入

multica desktop 没有 fork 一整套 SettingsPage，而是通过 desktop-only tabs 注入 Daemon / Updates。AgentDash desktop 化时，Settings、Backend Panel、MCP、accessible roots 也可以优先采用 slot/extension 方式，避免 web/desktop 页面分叉。

参考：`references/multica/apps/desktop/src/renderer/src/routes.tsx`。

### Workspace-scoped app tabs

multica desktop 的 tab store 按 workspace 隔离，每个 tab 有独立 memory router。AgentDash 现有 `workspaceTabStore` 更偏 session 右侧工作台。未来 desktop 可以保留两级 tab：app 级项目/页面 tab，与 session 内 workspace/tool tab 分开。

参考：`references/multica/apps/desktop/src/renderer/src/stores/tab-store.ts`、`frontend/src/stores/workspaceTabStore.ts`。

### Local status 双通道冲突提示

desktop 可同时看到 cloud relay status 和本机 IPC/local command status。两者短时间不一致时不一定是 bug，例如 cloud 尚未感知断连、本机刚重启、网络不可达但 local alive。UI 应标注状态来源，而不是合并成单一 online/offline。

参考：`references/multica/apps/desktop/src/renderer/src/platform/daemon-ipc-bridge.ts`。

### Streaming Markdown block memo

multica 的 streaming markdown 组件用 block split + memo 减少流式重绘。AgentDash session stream 高频事件更多，未来若出现长会话卡顿，可把该思路作为性能专项，而不是现在就引入复杂优化。

参考：`references/multica/packages/ui/markdown/StreamingMarkdown.tsx`、`frontend/src/features/session`。

## Product / Collaboration

### Trigger summary / context snapshot

multica task 保存 trigger summary，避免源评论、payload 或 issue 变更后丢失“为什么启动这次执行”。AgentDash ExecutionAttempt 未来可记录短摘要：触发者、触发源、目标 Task/Story、选用 Agent、workspace、关键输入摘要。

参考：`references/multica/server/migrations/061_task_trigger_summary.up.sql`、`references/multica/server/internal/service/task.go`。

### Comment 与 Agent reply 不必急着做

Comment/Agent reply 对协作体验有价值，但它依赖 Actor、Activity、Subscriber、Inbox。AgentDash 可以先做 Activity Timeline 和 ExecutionAttempt，等 Story/Task 协作语义更稳后，再引入人类/Agent 评论。

参考：`references/multica/server/internal/handler/comment.go`。

### Assignee frequency / Agent 推荐

multica 可从历史活动中推导常用 assignee。AgentDash 未来 Task 绑定 Agent 时，也可根据 ProjectAgentLink、历史 SessionBinding、Task 类型、workspace 维度推荐 Agent，但这属于体验增强，不应早于核心执行投影。

参考：`references/multica/server/pkg/db/queries/activity.sql`。

## Skill / Knowledge Asset

### Skill list/detail payload 分层

multica Skill list 不返回大 content，detail 再加载完整内容。AgentDash Skill Asset 若未来允许大量文件或 50KB 以上 skill，列表 API 应优先返回 summary，避免资产页越来越慢。

参考：`references/multica/server/internal/handler/skill.go`、`frontend/src/services/skillAsset.ts`。

### Local Skill import 的多节点状态问题

multica local skill import 明确区分 pending/running/completed/failed/timeout，并提醒多节点部署需要共享状态。AgentDash 若通过 local backend 盘点本机 skill，也要避免“请求发到 A 节点，状态轮询打到 B 节点”导致状态丢失。

参考：`references/multica/server/internal/handler/runtime_local_skills.go`。

## Analytics / Observability

### Usage rollup 先做最小事实，再做 dashboard

multica 有 task/runtime daily rollup。AgentDash 不应过早设计复杂 analytics，但可以在 ExecutionAttempt 中保留最小事实：started_at、finished_at、status、agent、backend/runtime、model、token summary、failure reason。等事实稳定后再做 dashboard rollup。

参考：`references/multica/server/pkg/db/queries/task_usage.sql`、`runtime_usage.sql`。

### Runtime quality score 可能来自多信号

未来 backend/runtime 质量不只看在线率，也可看任务成功率、平均启动时延、cancel/timeout 比例、MCP/tool 错误率、版本落后情况。该方向适合 dashboard 后期，不应在 Runtime Health 第一版塞太多指标。

## Testing / Engineering

### Query 文件作为产品行为索引

multica sqlc 查询文件很容易按产品行为阅读。AgentDash 不用改 sqlc，但复杂列表、统计、投影查询可以在 repository 层配对应集成测试，让“用户看到的列表为什么这样排序/过滤”更可验证。

参考：`references/multica/server/pkg/db/queries`、`crates/agentdash-infrastructure/src/persistence/postgres`。

### Provider adapter fixture 化

multica provider adapter 有不少针对 CLI 输出、stderr、usage、session id 的测试。AgentDash executor 后续可建立 provider output fixture，特别覆盖 Codex/Claude/Gemini/OpenCode 的异常输出和 resume 语义。

参考：`references/multica/server/pkg/agent`、`crates/agentdash-executor/src/connectors`。

## 路径与事实备忘

- 当前未发现 `research/multica-module-review.md`，README 已记录该缺口。
- 当前未发现 AgentDash 正式 desktop/Tauri/Electron 包；`pnpm dev` 是开发期联合启动，不等于产品化 desktop。
- multica 前端实际是 `packages/core` 与 `packages/views` 分离，不存在 `packages/views/core` 这一层。
- `references/multica/server/internal/daemon/repocache/cache.go` 是 repo cache 关键入口。
