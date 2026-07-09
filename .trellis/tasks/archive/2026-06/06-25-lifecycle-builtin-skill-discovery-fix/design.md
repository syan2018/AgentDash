# Design

## Scope

本任务同时修复两个问题：

- lifecycle 内置 Skill 的 runtime bootstrap 与 projection 没有标准化，`EnsureAndProject` 只写 metadata，不创建/同步项目级 builtin SkillAsset。
- VFS-first SkillDiscoveryProvider 的扫描入口在 crate split 后没有接回 composition owner，导致 provider 声明 VFS rules 时被 `runtime_capability_projection` 跳过。

本任务纳入一个小型物理拆分：新增 `agentdash-application-skill` crate。原因是 skill 应用层同时被 lifecycle projection、AgentRun capability projection、总 application API/路由使用；继续放在 `agentdash-application` 会让 `agentdash-application-lifecycle` 无法调用标准 bootstrap，只能保留 companion 特例或绕回总 application。

## Target Crate Boundary

`agentdash-application-skill` 负责：

- builtin skill template registry：从 domain embedded bundles 映射到 `BuiltinSkillAssetTemplate`。
- project SkillAsset service：create/update/reset/import/bootstrap 现有能力迁入或 re-export 到新 crate。
- skill file parser/loader：`parse_skill_file`、frontmatter validation、local dir scan、VFS scan。
- runtime skill baseline projection：把 VFS scan、VFS-first provider output、legacy provider output、local dir scan 归一为 `SessionBaselineCapabilities`。

`agentdash-domain` 继续负责：

- `EmbeddedSkillBundle`、`SkillAsset`、repository traits 和各业务域 embedded bundle 常量。

`agentdash-application-vfs` 继续负责：

- lifecycle / skill_asset mount metadata、projected skill file read/list/search。
- generic VFS file discovery primitive 可以先留在总 `agentdash-application` 或迁入 skill crate；实现时以最小依赖闭包为准。如果 skill crate 需要 VFS-first provider 扫描，应把 skill 专用扫描函数迁入 skill crate，避免反向依赖总 application。

`agentdash-application-lifecycle` 负责：

- 根据 `BuiltinLifecycleSkillPolicy` 调用 skill crate 的 builtin bootstrap service。
- 只在有效 SkillAsset key 已确保存在后刷新 lifecycle mount projection metadata。

`agentdash-application-agentrun` 负责：

- 持有 AgentRun frame / runtime projection primitives。
- 不再把 VFS-first provider 视为不可扫描；如果 baseline projection 迁出，则通过 skill crate 获取完整 baseline。

总 `agentdash-application` 负责：

- 组合 frame construction、API-facing service re-export 和跨模块 wiring。
- 删除 `companion::skill_projection` 的单点 ensure 流程，调用 lifecycle surface 标准 policy。

## Runtime Data Flow

```text
Frame construction / lifecycle surface caller
  -> BuiltinLifecycleSkillPolicy::EnsureAndProject([...])
  -> AgentRunLifecycleSurfaceProjector
  -> agentdash-application-skill::SkillAssetService::bootstrap_builtins(project_id, key)
  -> lifecycle mount metadata: skill_asset_project_id + skill_asset_keys
  -> LifecycleMountProvider reads lifecycle://skills/<key>/...
  -> skill baseline projection scans active VFS / provider VFS rules / local dirs
  -> CapabilityState.skill.skills
  -> runtime context frame / connector context
```

## VFS-First Discovery Repair

Provider `vfs_discovery_rules()` 不应在 baseline projection 中直接跳过。正确形态：

- 有 VFS rules 的 provider：由 composition owner 使用 active VFS + `VfsService` 扫描文件，调用 provider 的 VFS output ingest path 或等价转换逻辑，生成 `SkillDiscoveryOutput`。
- 无 VFS rules 的 provider：继续调用 `provider.discover(context)`。
- diagnostics 只记录实际扫描或 provider 失败，不再把正常 VFS-first provider 标为 `vfs_scanner_unavailable`。

如果现有 SPI 缺少“provider 消费 VFS 文件”的明确方法，优先使用现有 `discover_vfs_files` 默认扩展点；若命名或职责不清，补强 trait 默认方法，但保持 connector/integration 兼容在源码级同步更新，不做运行时 fallback。

## Builtin Lifecycle Skill Policy

`EnsureAndProject` 的语义必须包含两步：

1. ensure：对每个 builtin skill key 调用 `bootstrap_builtins(project_id, Some(key))`，同步 embedded bundle 到项目 SkillAsset。
2. project：将 explicit skill keys 与 builtin keys 合并去重，刷新 lifecycle mount projection metadata。

`PreserveProjected` 只保留已有 lifecycle mount projection，不触发 bootstrap。

## Crate Split Decision

结论：本任务直接纳入 `agentdash-application-skill`。

理由：

- lifecycle crate 已持有 `SkillAssetRepository`，但没有 SkillAsset bootstrap 逻辑；现有未读字段说明边界已经准备好却缺 implementation。
- agentrun crate 需要 skill baseline projection，当前把 VFS scan 移走后没有标准 owner 接回；skill crate 能提供可复用 projection service。
- companion-only helper 是边界缺失的症状，不是应该继续扩展的模式。
- 拆出 skill crate 的依赖方向清晰：`application-skill -> application-vfs/domain/spi`，`application-lifecycle/agentrun/application -> application-skill`，不会形成循环。

Trade-off：

- 本任务改动面会大于只修 helper，但能一次性修正事实源和依赖方向。
- 暂不把 API DTO、infrastructure repository、domain entity 搬入 skill crate；它们已在各自层中归位。

## Migration Notes

已有 migration `0030_builtin_skill_runtime_ownership.sql` 保留。代码修复后，新项目/旧项目都应通过 runtime bootstrap 同步 builtin SkillAsset；不新增兼容 fallback。

## Tests

需要覆盖：

- skill crate 的 builtin bootstrap 和旧快照收敛。
- lifecycle projector `EnsureAndProject` 真实创建/同步 SkillAsset 并投影 metadata。
- lifecycle VFS `skills/canvas-system/SKILL.md` 可读、可 list，并能进入 baseline。
- VFS-first provider 从 active VFS 发现 skill，不产生 `vfs_scanner_unavailable`。
- owner bootstrap / companion child / routine / workspace module 关键路径不再依赖 companion-only helper。
