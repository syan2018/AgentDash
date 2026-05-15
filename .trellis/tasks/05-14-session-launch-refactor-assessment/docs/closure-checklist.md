# Closure Checklist

## Main Data Flow

- [ ] 所有生产启动来源只构造 `LaunchCommand`。
- [ ] `LaunchCommand` 只包含来源意图和引用，不包含 resolved VFS/MCP/capability/context/hook/effect/working_dir/connector input。
- [x] `UserPromptInput` 不包含 `working_dir`。
- [x] 生产主链路不存在 `PromptSessionRequest`、`PreparedSessionInputs`、`finalize_request`、`PreparedLaunchPrompt`、`SessionLaunchPlan`、`AugmentedLaunchInput`。
- [x] `PromptAugmentInput` 不作为跨 crate handoff、planner 输入或增强后输出存在。
- [x] `SessionLaunchRequest` 过渡 envelope 已删除。
- [x] `SessionConstructionFacts` provider handoff 已删除，construction / launch / effects 字段分别进入目标边界。
- [ ] `SessionConstructionPlan` 是 owner/workspace/VFS/MCP/capability/executor/context/identity 的唯一事实源。
- [ ] `LaunchExecution` 是 lifecycle/restore/hook/follow-up/runtime-command/terminal-effect/connector-input 的唯一 per-launch 计划。
- [ ] `ExecutionContext` 只在 connector 边界投影生成。

## Source Adapters

- [ ] HTTP adapter 不构造 VFS/MCP/context/capability。
- [x] Task adapter 不传 `post_turn_handler` trait object；task effect binding 已进入 construction/effects durable binding。
- [ ] Companion adapter 不传 parent VFS/MCP/context snapshot；construction 从 parent session facts 解析 slice。
- [ ] Local relay adapter 只传 workspace root 和原始 MCP declaration，不传 resolved VFS/MCP/capability。
- [ ] Hook auto-resume strict 复用主 construction/launch 路径。

## Construction / Context / Owner

- [ ] owner 解析只通过 `SessionOwnerResolver` / `ResolvedSessionOwner`。
- [ ] working dir 由 construction 根据 owner/workspace/agent/lifecycle/local relay root 解析。
- [ ] launch、context endpoint、权限展示、audit/inspector 使用同一 owner 语义。
- [ ] context endpoint、audit、inspector 只投影 `SessionConstructionPlan`。
- [x] route/bootstrap 不保留 task/story/project context 主线重建。
- [ ] launch 与 context endpoint 的 VFS/capability/context 有一致性测试。

## Runtime / Pipeline

- [x] `SessionLaunchPlanner` 消费 `LaunchCommand + SessionConstructionPlan + runtime facts`。
- [x] `prompt_pipeline` 只负责执行计划：turn claim/activate、event append、connector.prompt、accepted 后提交、processor supervision。
- [x] connector.prompt 失败不会提交 bootstrap、pending applied、title generation 等成功副作用。
- [ ] runtime registry 与 turn supervisor 是 active turn / cancel / stall 的入口。
- [ ] connector live executor session 与 app active turn 命名和查询分离。

## Effects / Pending / Persistence

- [ ] terminal event 先持久化，effect 后进入 durable outbox。
- [ ] 所有 terminal effect handler 具备 durable identity 或 typed handler。
- [ ] effect 支持 retry、dead-letter、replay 与审计。
- [ ] pending runtime command 不藏在 `SessionMeta` 普通字段。
- [ ] pending command 有 requested/applied/failed 审计与 apply-once 测试。
- [ ] PostgreSQL 与 SQLite migration 覆盖旧字段删除/迁移。
- [ ] 新增业务逻辑按 meta/event/outbox/runtime-command store 边界依赖，不绕回大 `SessionPersistence`。

## SessionHub / AppState / Path

- [ ] `SessionHub` 不再是业务能力入口。
- [ ] 若 `SessionHub` 类型仍存在，已确认它不承载业务判断且不作为最终完成遮羞布。
- [ ] AppState ready 后 session construction provider、audit bus、terminal callback/tool provider/effect handlers 不为空。
- [ ] resolved working directory 只接受 mount root 内规范化相对路径。
- [ ] path policy 测试覆盖绝对路径、`..`、Windows prefix/root、空 segment。

## Final Validation Matrix

- [ ] `cargo fmt --check`
- [ ] `cargo check -p agentdash-application`
- [ ] `cargo check -p agentdash-api`
- [ ] `cargo check -p agentdash-infrastructure`
- [ ] `cargo check -p agentdash-local`
- [ ] `cargo test -p agentdash-application session::launch`
- [ ] `cargo test -p agentdash-application session::construction`
- [ ] `cargo test -p agentdash-application session::hub`
- [ ] `cargo test -p agentdash-application session::terminal_effects`
- [ ] `cargo test -p agentdash-application session::runtime_commands`
- [ ] `cargo test -p agentdash-application session::memory_persistence`
- [ ] `cargo test -p agentdash-application session::path_policy`
- [ ] `cargo test -p agentdash-infrastructure terminal_effect_outbox_persists_status_transitions`
- [ ] `git diff --check`
