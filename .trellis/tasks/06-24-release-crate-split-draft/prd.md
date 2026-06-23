# Release crates 拆分草案

## Goal

基于 release 前模块边界调研，先保存 crates 拆分目标、候选 crate 图、依赖方向和前置条件；不立即实施物理 crate extraction。

本任务是 crate split draft holder。它只定义目标、候选图、依赖方向、前置条件和未来 extraction waves；不在当前阶段修改 Rust crate 布局。

## Context

- 父任务：`.trellis/tasks/06-24-release-crate-boundary-review`
- 前置实施任务：`.trellis/tasks/06-24-agentrun-runtime-session-decoupling`
- 当前判断：Cargo crate graph 不是主要问题，`agentdash-application` 内部 broad facade 和双向 imports 才是 physical split 的阻塞点。

## Requirements

- 记录 release 目标 crate/module 图，明确每个候选 crate 的职责和不应包含的内容。
- 明确 physical extraction 前置条件：AgentRun/RuntimeSession/Lifecycle/RuntimeGateway/VFS resource facade 解耦完成，public exports 收紧，API route 不依赖 session internals。
- 保存推荐 extraction order：先扩展 `agentdash-application-ports`，再 RuntimeSession / RuntimeGateway，再 AgentRun / Lifecycle，VFS 延后。
- 明确每个 wave 的 compile/test gates 和不能启动的阻塞条件。
- 本任务保持 planning/draft；不启动实现、不创建 migration、不移动 Cargo workspace members。

## Acceptance Criteria

- [ ] `design.md` 保存候选 crate 图、职责、依赖方向和 extraction waves。
- [ ] `implement.md` 保存未来 crate split 前置检查和 wave checklist。
- [ ] 明确 crate split 依赖 `06-24-agentrun-runtime-session-decoupling` 完成或至少完成其 facade/import cleanup 阶段。
- [ ] 不修改生产源码或 Cargo workspace 配置。

## Notes

- 这不是当前 release 立刻开工的 extraction 任务，而是避免后续重新争论方向的 draft。
