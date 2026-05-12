# 云端 Skill 资产仓储与 Agent 装载

## Goal

为 Project 提供一套完整的云端 Skill 资产模型：平台可仓储和装载 Skill，用户可通过浏览器上传本地 Skill ZIP/目录并更新，内嵌 Skill 以项目种子资产方式 bootstrap 且可编辑，Agent preset 可选择装载这些 Skill。运行时通过 VFS 虚拟映射暴露为 `skills/<skill-key>/...`，继续复用现有 `load_skills_from_vfs` 与 `CapabilityState.skill` 链路。

## Requirements

- 新增 project scoped `SkillAsset` 领域聚合、仓储、PostgreSQL migration 与应用服务，存储 metadata 和完整文件集。
- Skill 校验与现有 loader 保持一致：`key/name` 只允许小写字母、数字、连字符，最长 64；必须有 `SKILL.md`；frontmatter `name` 必须等于 asset key；`description` 必填且最长 1024；文件路径必须是安全相对路径。
- 新增 `/api/projects/{project_id}/skill-assets` 系列 API：list/get/create/update/delete/upload/bootstrap/reset-from-builtin；读走 Project View 权限，写走 Project Edit 权限。
- 上传第一版支持浏览器 ZIP 与目录 multipart；VFS/Workspace import 不在本任务内。
- 复用 `EmbeddedSkillBundle` 注册内嵌 Skill 模板；bootstrap 后成为 `source=builtin_seed` 的项目种子资产，项目编辑者可直接修改；重复 bootstrap 不覆盖已编辑内容，显式 reset 才恢复源码模板。
- `AgentPresetConfig` 新增 `skill_asset_keys`，合并规则与 `mcp_preset_keys` 一致；Agent preset 编辑器可选择项目 Skill。
- Session 装配阶段根据 Agent 的 `skill_asset_keys` 追加只读 Skill VFS projection；有 lifecycle mount 时暴露在 `lifecycle://skills/<key>/...`，无 lifecycle mount 时暴露在 `skill-assets://skills/<key>/...`。
- Assets 页 Skill 类目改为后端 API 驱动，支持列表、上传、创建、编辑 `SKILL.md` 与附加文件、删除、bootstrap 内嵌 Skill、reset builtin seed。

## Acceptance Criteria

- [ ] 能在 Assets / Skill 页面创建、上传、编辑、删除项目级云端 Skill。
- [ ] 能 bootstrap 内嵌 Skill 为项目种子资产，编辑后再次 bootstrap 不覆盖，reset 后恢复模板。
- [ ] Agent preset 能选择 Skill，保存后配置中出现 `skill_asset_keys`。
- [ ] 启动选择了 Skill 的 Agent session 后，现有 Skill scanner 能发现云端 Skill，并写入 `CapabilityState.skill.skills`。
- [ ] lifecycle session 中选中 Skill 可通过 `lifecycle://skills/<key>/SKILL.md` 读取；普通 session 中可通过 `skill-assets://skills/<key>/SKILL.md` 读取。
- [ ] Project View/Edit 权限分别约束读取和写入 API。
- [ ] 后端单测/仓储测试/API 边界测试/VFS 发现测试覆盖核心路径；前端 typecheck 通过。

## Technical Approach

- 后端新增 `agentdash-domain::skill_asset` 模块，定义 `SkillAsset`、`SkillAssetFile`、`SkillAssetSource`、`SkillAssetRepository` 与路径/文件校验辅助。
- 基础设施层新增 `PostgresSkillAssetRepository` 与 `0029_skill_assets.sql`；应用层新增 `SkillAssetService`，负责 CRUD、上传归一化、builtin bootstrap/reset 和 loader-compatible frontmatter 同步。
- API 层新增 `dto::skill_asset` 与 `routes::skill_assets`，参考 `mcp_presets` 的 project scoped 权限模式，但 builtin seed 允许直接编辑。
- VFS 层新增 `skill_asset_fs` provider，按 mount metadata 中的 `project_id` 与 `skill_asset_keys` 只读投影文件；扩展 lifecycle provider 支持 `skills/` 路径族，或在构建 lifecycle mount 时附带 skill metadata 让同一 provider 读取 Skill 资产。
- Session 装配层在构造 VFS 后依据 effective agent config 的 `skill_asset_keys` 追加 skill projection；不要把 Skill 纳入 Tool Capability directive。
- 前端新增 SkillAsset DTO/service，改造 Assets Skill panel 和 Agent preset editor，使用后端 API 而非直接写 project inline context container。

## Out of Scope

- 不做从 VFS/Workspace 直接 import Skill。
- 不保留 project inline context container 作为 Skill 资产最终模型。
- 不新增平台全局 Skill 管理权限；内嵌 Skill bootstrap 后按 Project 权限编辑。
- 不引入新的 prompt 注入协议；Skill runtime 继续走 VFS scanner。

## Technical Notes

- 现有 `crates/agentdash-application/src/skill/loader.rs` 已扫描 `.agents/skills/*/SKILL.md` 与 `skills/*/SKILL.md`，应尽量复用。
- 现有 `EmbeddedSkillBundle` 在 `agentdash-domain/src/embedded_skill.rs`，Canvas 已有 bundle 用例。
- `MountLink` 当前只在 `parse_mount_uri` 路径解析中生效，`discover_mount_files` 直接 read/list mount/path，因此本任务不依赖 link 来实现 Skill projection。
- 之前误生成的 `frontend/src/features/assets-panel/categories/SkillCategoryPanel.tsx` 与 `frontend/src/services/skillAsset.ts` 是 inline-container 草稿，实现时应改造为 API client/UI，而不是沿用存储模型。
- Canvas System 当前与 Canvas 文件模型深度绑定，本任务不强行迁移其既有注入方式；同时将其注册为 builtin SkillAsset 模板，供项目级 Skill 仓储 bootstrap / reset 使用。

## Definition of Done

- Tests added/updated for backend service, repository, API permissions, VFS discovery, Agent config roundtrip, and frontend type safety.
- `cargo test` 相关 crate 通过，`pnpm typecheck` 通过。
- 如实现中沉淀新约定，更新 `.trellis/spec/`。
- 提交信息符合 `type(scope): 中文提交信息`，并在 commit body 分点描述具体更新。
