# 修复 lifecycle 内置 Skill 注入与 VFS-first 发现链路

## Goal

修复 AgentRun lifecycle mount 上的内置 Skill 注入和 Skill 发现链路，使 `canvas-system`、`workspace-module-system`、`companion-system`、`routine-memory` 等源码内嵌 Skill 通过同一套标准 runtime bootstrap + lifecycle projection + VFS-first discovery 流程进入 Agent 可见 skill baseline / context frame。

本任务要把 `e41f3e26 fix(skill): 收束内置 Skill 运行时归属` 引入的新归属模型补完整：内置系统 Skill 不再作为 Shared Library marketplace seed 安装，但 runtime 需要直接同步项目级 builtin SkillAsset，并通过 lifecycle mount 暴露给 session。

## Confirmed Facts

- `SkillAssetService::bootstrap_builtins(project_id, Some(key))` 已能从 embedded bundle 创建或同步项目级 builtin SkillAsset，并能收敛旧市场安装快照。
- `BuiltinLifecycleSkillPolicy::EnsureAndProject` 当前只把 builtin skill key 写入 lifecycle mount metadata，没有调用 SkillAsset bootstrap；`AgentRunLifecycleSurfaceProjector.skill_asset_repo` 编译时已显示为未读取字段。
- `owner_bootstrap` 里存在 `ensure_companion_system_skill_asset` 这条 companion-only 特例，因此 `companion-system` 和其它 builtin skill 的注入路径不一致。
- `runtime_capability_projection` 当前遇到声明 `vfs_discovery_rules()` 的 provider 会记录 `vfs_scanner_unavailable` 并跳过；VFS-first provider 的扫描没有在 application composition owner 中补回。
- `skill::loader::load_skills_from_vfs` 和 `context::mount_file_discovery` 仍保留 VFS skill 文件扫描能力，但 frame construction 当前没有把这套扫描结果接入 skill baseline。

## Requirements

- lifecycle 内置 Skill 注入必须有统一入口：调用方声明 builtin skill keys / policy 后，由标准流程同步项目级 builtin SkillAsset，并把有效 keys 投影到 lifecycle mount。
- `EnsureAndProject` 的语义必须名实一致：确保内置 SkillAsset 存在且内容与 embedded bundle 同步，然后再投影。
- `ensure_companion_system_skill_asset` 这类 companion-only 注入路径应被统一机制替代，companion 不再拥有独立特殊流程。
- Skill 应用层能力应物理收束到独立 crate：本任务纳入 `agentdash-application-skill`，承载 builtin SkillAsset bootstrap、Skill 文件解析/加载、VFS-first provider 扫描和 baseline projection，避免 lifecycle / agentrun 通过总 `agentdash-application` 或特例 helper 互相绕依赖。
- VFS-first SkillDiscoveryProvider 必须恢复为可用：provider 声明 VFS discovery rules 时，由 application composition owner 执行 VFS 扫描，再把发现结果纳入 session skill baseline。
- lifecycle mount 上的 `skills/<key>/SKILL.md` 与 references 必须能被 discovery 识别，并生成 Agent 可见 skill baseline / skill context frame。
- `canvas-system` 必须在具备 canvas / workspace module runtime surface 的 AgentRun 中可见；`workspace-module-system`、`companion-system`、`routine-memory` 的行为也必须走同一基础设施。
- Shared Library marketplace 不再承载这些内置系统 Skill 的用户安装入口；runtime bootstrap 是内置系统 Skill 的事实来源。
- 修复应保留项目级 SkillAsset 管理 surface 的可读性与 reset/sync 能力。

## Acceptance Criteria

- [x] 新增或调整测试证明 `EnsureAndProject([CanvasSystem, WorkspaceModuleSystem, CompanionSystem])` 会创建/同步对应项目级 builtin SkillAsset，并在 lifecycle mount metadata 中投影相同 keys。
- [x] 新增或调整测试证明 `companion-system` 通过统一 builtin lifecycle skill 流程进入 projection，不依赖 companion-only ensure helper。
- [x] 新增或调整测试证明 lifecycle VFS 中的 `skills/canvas-system/SKILL.md` 能进入 session skill baseline，且 baseline 包含正确 capability key / file path / base dir。
- [x] 新增或调整测试证明 VFS-first provider 不再被 `vfs_scanner_unavailable` 跳过，而是能基于 active VFS discovery rules 产出默认暴露 skills。
- [x] `agentdash-application-skill` 进入 workspace，`agentdash-application-lifecycle` 和 `agentdash-application-agentrun` 通过该 crate 消费标准 skill bootstrap/discovery 能力；总 `agentdash-application` 不再持有这些底层 skill 实现作为唯一入口。
- [x] 现有 `skill_asset::service::builtin_bootstrap_*` 测试继续通过，旧市场安装快照仍会收敛为 builtin seed SkillAsset。
- [x] `cargo test` 覆盖被修改的 Rust crates 中相关单元测试；至少包含 application / application-lifecycle / application-agentrun 中受影响的 skill 或 lifecycle projection 测试。

## Notes

- 根因初查来自 2026-06-25 会话：`e41f3e26` 移除 Shared Library builtin SkillTemplate seed 后，未完整补上 runtime direct bootstrap；更早的 crate split / VFS-first 迁移已让 VFS-rule provider 扫描在 AgentRun crate 中被跳过。
- 本任务直接纳入 skill 应用层 crate 拆分，因为现有依赖已经证明 lifecycle 和 agentrun 都需要同一套 skill 能力，而继续放在总 `agentdash-application` 会制造反向依赖或特例 helper。
