# Lifecycle mount 架构清理

## Goal

收束 AgentRun lifecycle mount 的构建与投影边界，让运行证据面、节点执行面、SkillAsset projection 和 VFS browser 共享同一条 application 事实源。

## Requirements

- `AgentRunLifecycleSurfaceProjector` 是 AgentRun lifecycle surface 的唯一业务入口。
- AgentRun workspace query、VFS surface resolver、owner bootstrap、session assembler 只能收集上下文事实并调用 projector 场景化入口，不能自行拼 mount metadata 或直接调用低层 builder。
- 低层 mount builder 只负责从 typed projection facts 生成 mount，不承担业务入口选择、builtin skill ensure 或跨来源 metadata 合并策略。
- `lifecycle` mount 的 session evidence scope 与 node runtime scope 必须由明确 projection mode 决定，不能由是否存在 node coordinate 隐式切换。
- SkillAsset projection、message stream projection、node anchor projection 必须来自同一份结构化 facts，避免浏览入口、执行入口和 workspace query 看到不同 surface。
- VFS overlay / mount directive 的整 mount replace 语义需要和 lifecycle projection 局部刷新语义分离。
- 清理完成后，代码中不应存在多个可被业务层直接调用的 AgentRun lifecycle mount 重建入口。
- 只读 AgentRun evidence 中的 node reference 不能复用可写 node runtime projection 类型。
- 全局 legacy cleanup 作为本任务下的 work items 管理，不创建 Trellis 子任务；每个 work item 用独立文件记录目标、范围、依赖和验证。

## Confirmed Findings

- 当前 workspace query、VFS surface resolver、owner bootstrap、session assembler 都会进入 lifecycle surface projection，但收集的 facts 不一致。
- VFS surface resolver 目前能构造不完整 node projection：`lifecycle_key` 与 writable port keys 为空，但该类型本来表达可写 node runtime scope。
- `append_lifecycle_skill_asset_projection` 修复了 skills 目录可见性，但它仍是 rebuild 后补 metadata 的形态；目标形态应由 projector 一次性生成完整 provider metadata。
- `build_agent_run_session_lifecycle_mount`、`build_lifecycle_mount_with_node_scope` 及 wrapper 仍通过 `vfs` 模块公开，业务层仍能绕过 projector。
- 全局 legacy 扫描发现多项独立清理候选，其中部分是拒绝旧 schema 的守卫测试，不应作为“断旧测试”删除。

## Acceptance Criteria

- [x] AgentRun workspace query、VFS surface resolver、owner bootstrap、session assembler 的 lifecycle surface 构建路径归并到统一 projector 或明确的 projector 子入口。
- [x] `lifecycle` mount 替换不会丢失同 Project 的 SkillAsset projection、message stream projection 或 node anchor projection。
- [x] node runtime mount 和 AgentRun session evidence mount 的路径集合有测试覆盖，且不会因同一个 anchor 同时存在 session 与 node facts 而混淆。
- [x] 不再通过 public helper 暴露裸 AgentRun lifecycle mount 重建能力。
- [x] 相关 backend spec 记录最终 owner、projection facts 和 replace/merge 边界。
- [x] VFS surface resolver 不再通过空 `lifecycle_key` / 空 writable ports 构造可写 node projection。
- [x] `work-items/` 中列出的 legacy cleanup 项完成时，必须连同对应生产数据结构、调用点和测试一起清理；拒绝旧 schema 的守卫测试不能被误删。

## Notes

- 该任务是架构清理，不要求保留旧 surface 路径兼容。
- legacy cleanup 不拆 Trellis 子任务，但按 work item 文件维护独立验证边界。
