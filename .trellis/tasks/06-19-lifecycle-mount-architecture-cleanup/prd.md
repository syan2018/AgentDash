# Lifecycle mount 架构清理

## Goal

收束 AgentRun lifecycle mount 的构建与投影边界，让运行证据面、节点执行面、SkillAsset projection 和 VFS browser 共享同一条 application 事实源。

## Requirements

- `AgentRunLifecycleSurfaceProjector` 是 AgentRun lifecycle surface 的唯一业务入口。
- 低层 mount builder 只负责从 typed projection facts 生成 mount，不承担业务入口选择、builtin skill ensure 或跨来源 metadata 合并策略。
- `lifecycle` mount 的 session evidence scope 与 node runtime scope 必须由明确 projection mode 决定，不能由是否存在 node coordinate 隐式切换。
- SkillAsset projection、message stream projection、node anchor projection 必须来自同一份结构化 facts，避免浏览入口、执行入口和 workspace query 看到不同 surface。
- VFS overlay / mount directive 的整 mount replace 语义需要和 lifecycle projection 局部刷新语义分离。
- 清理完成后，代码中不应存在多个可被业务层直接调用的 AgentRun lifecycle mount 重建入口。

## Acceptance Criteria

- [ ] AgentRun workspace query、VFS surface resolver、owner bootstrap、session assembler 的 lifecycle surface 构建路径归并到统一 projector 或明确的 projector 子入口。
- [ ] `lifecycle` mount 替换不会丢失同 Project 的 SkillAsset projection、message stream projection 或 node anchor projection。
- [ ] node runtime mount 和 AgentRun session evidence mount 的路径集合有测试覆盖，且不会因同一个 anchor 同时存在 session 与 node facts 而混淆。
- [ ] 不再通过 public helper 暴露裸 AgentRun lifecycle mount 重建能力。
- [ ] 相关 backend spec 记录最终 owner、projection facts 和 replace/merge 边界。

## Notes

- 该任务是架构清理，不要求保留旧 surface 路径兼容。
