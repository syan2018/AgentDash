# Project Filespace 资产化与 Agent VFS 能力分配迁移

## Goal

把当前 Project 级 `inline_files` VFS 空间从 Project Settings 中迁出，收敛为 Project Assets 中的一等 Project Filespace 资产，并支持显式发布/安装到 Shared Library / Marketplace。

同时将 Project 级 VFS 访问纳入 Project Agent 的能力设置，使 Agent 能像选择 MCP Preset、Skill Asset、Tool capability 一样，按 Project Filespace / mount / capability 粒度配置 VFS 实际访问权限。Agent 的 VFS capability 不是只控制“是否可见”，而是参与最终 mount table 计算，修改每个 VFS mount 在该 Agent session 中的有效 capabilities。

Story 级 inline VFS 不参与资产化迁移。它是 Story 局部上下文绑定，与 Story 生命周期高度绑定，只在最终 Session VFS 中作为 Story overlay 追加或覆盖。

## User Value

- Project Settings 不再承载日常文件资产维护，设置页只保留 runtime preview / 诊断入口。
- 用户能在 Assets 中集中管理可复用项目文件空间，例如项目说明、约束、Prompt 片段、示例、图片或其它 inline 文件。
- 用户能把经过整理的 Filespace 显式发布到 Marketplace，并在其它 Project 中安装成可编辑副本。
- Project Agent 能细粒度控制项目级 VFS 空间的实际访问权限，避免所有 Agent 默认以同等权限看到所有项目上下文。
- Session runtime 的 VFS 装配来源更清晰：Project 可复用资产、Story 局部上下文、Agent 能力选择各自承担独立职责。

## Confirmed Facts

- 当前 Project / Story VFS 配置共用 `ContextContainerDefinition`，支持 `inline_files` 与 `external_service` 两种 provider。
- Project 级容器保存在 `project.config.context_containers`，Story 级容器保存在 `story.context.context_containers`。
- Story 支持 `disabled_container_ids` 禁用继承的 Project 容器，并支持同名 mount 覆盖 Project 容器。
- `build_derived_vfs` 当前按 Workspace → effective context containers 的顺序构建最终 VFS。
- `ProjectAgent.project_container_ids` 已存在，语义是 Project 级容器白名单；该字段没有前端配置入口，实际未形成用户可见能力。
- 现有 `filter_project_containers_by_whitelist` 按 ProjectAgent 白名单过滤 Project-owned context container mount。
- `inline_fs_files` 已经是独立文件内容表，并支持 text / binary typed storage。
- Shared Library 当前支持 `agent_template`、`mcp_server_template`、`workflow_template`、`skill_template`、`extension_template`。
- Shared Library 规范要求 Project 运行时只读取安装后的 Project 资源，不直接消费 `LibraryAsset.payload`。
- Assets 页当前已承载 Workflow / MCP / Skill / Canvas，Marketplace 通过 Project 资源发布/安装。

## Requirements

### Project Filespace Asset

- 新增 Project Filespace 领域模型，作为 Project 内可运行、可编辑的文件空间资产。
- Project Filespace 必须有稳定 key、display name、description、source / installed source metadata、created / updated timestamps。
- Filespace 文件内容必须复用 `inline_fs_files` 的 typed embedded file storage，不新增第二套文件内容存储。
- Filespace 文件应支持 text 与 binary metadata，至少不破坏当前 VFS Browser 对 inline_fs 图片 / 二进制内容的支持。
- Project Filespace 在 Assets 页拥有独立类目，支持列表、创建、编辑、删除、浏览文件、上传图片或文件、发布到 Marketplace。
- Project Settings 中不再创建 Project 级 inline files 容器，只展示解析后的 VFS runtime preview 和跳转入口。

### VFS Mount Binding

- Project 级 VFS mount 配置应拆分为 mount binding，而不是直接内联文件内容。
- Project mount binding 表达：
  - mount id
  - display name
  - provider / source kind
  - Project Filespace asset reference 或 external service root ref
  - capabilities
  - default_write
  - owner scope
- Project Filespace asset 与 mount binding 生命周期分离：删除 binding 不删除 Filespace，删除 Filespace 必须校验或处理引用它的 binding。
- Story 级 inline container 保持现状，不迁移为 Project Filespace asset。
- Story `disabled_container_ids` 与同名覆盖语义保留。

### Agent VFS Capability

- Project Agent 配置必须支持 VFS access policy。
- 新建 Project Agent 默认不授予任何 Project Filespace / Project VFS mount 权限；用户必须显式添加。
- VFS access policy 至少支持按 Project mount / Filespace 设置有效访问权限。
- VFS access policy 必须支持 capability 粒度，至少覆盖 read / write / list / search；exec 仍受 provider 与 mount capability 约束。
- VFS access policy 产物必须写入最终 `Vfs.mounts[].capabilities`，Agent runtime 和 VFS tools 只看到裁剪后的有效权限。
- 未授予任何 capability 的 Project Filespace mount 不进入 Agent runtime surface；授予部分 capability 的 mount 必须以收窄后的 capabilities 进入 runtime surface。
- VFS access policy 只能在 Project mount binding/provider 支持能力范围内收窄权限，不能越权增加底层 mount 不支持的能力。
- 迁移直接退役旧 `project_container_ids` 字段，不需要映射为新 VFS access policy；新策略以显式配置为准。
- Session Construction 的最终 VFS 必须经过 Agent VFS access policy 解析，得到 per-Agent effective mount table。
- Story 局部 inline VFS 不受 Project Agent Filespace 资产白名单迁移影响；它作为 Story 局部上下文参与最终叠加。

### Shared Library / Marketplace

- 新增 Shared Library asset type：`filespace_template`。
- Project Filespace 可显式发布为 FilespaceTemplate；前端发布请求不得传 raw payload。
- 后端发布时必须读取 Project Filespace 权威状态并生成 typed payload。
- FilespaceTemplate 第一版必须直接支持 text / binary 文件，不为 binary 文件设计特殊排除或后续兼容路径。
- Binary 文件 payload 必须保留 `content_kind`、`mime_type`、`size_bytes` 与 base64 内容，安装后还原为 `inline_fs_files` 的 binary typed content。
- Marketplace 安装 FilespaceTemplate 后必须创建 Project Filespace 可编辑副本，并记录 `InstalledAssetSource`。
- 安装 FilespaceTemplate 后不应自动影响 Agent runtime；是否挂载由后续 Project mount binding 或安装向导明确决定。

### Migration

- 数据库 migration 只迁移 Project 级 `context_containers[].provider.inline_files`。
- 每个 Project 级 inline container 迁移为：
  - 一个 Project Filespace asset；
  - 一个引用该 Filespace 的 Project mount binding；
  - 对应文件内容迁入或重归属到 `inline_fs_files` 的 Filespace owner。
- Project 级 `external_service` container 不迁移为 Filespace asset，但应迁入 Project mount binding。
- Story 级 `context_containers` 保持现有结构或仅做最小字段适配，不生成 Project Filespace asset。
- `ProjectAgent.project_container_ids` 直接从 DB / domain / DTO / 前端类型中移除；迁移后运行路径不再依赖旧字段。
- 项目处于预研期，迁移后不保留旧模型运行分支。

## Acceptance Criteria

- [ ] Project Settings 不再提供 Project inline VFS 创建/编辑入口，只保留 VFS preview / 诊断 / 跳转。
- [ ] Assets 中出现 Project Filespace 类目，并能完成 Filespace CRUD 与 VFS Browser 文件编辑。
- [ ] Project 级 inline container 数据完成 migration，迁移后文件内容、mount id、display name、capabilities、default_write 保持可观察等价。
- [ ] Story 级 inline VFS 未被迁移为 Project asset，Story override / disable inherited mount 行为保持可用。
- [ ] Project Agent 编辑器能为每个 Project VFS mount / Filespace 配置 read / write / list / search 等有效权限。
- [ ] Session runtime VFS 只包含当前 Agent 获准访问的 Project Filespace mounts，并且每个 mount 的 `capabilities` 是 Agent access policy 计算后的有效权限。
- [ ] `fs.read` / `fs.write` / `fs.list` / `fs.search` 等 VFS tools 基于裁剪后的 mount capabilities 判定权限，而不是绕过 Agent VFS access policy 读取原始 Project mount binding。
- [ ] 旧 `project_container_ids` 已从 DB / domain / DTO / 前端类型和运行路径中移除，不生成默认 VFS grants。
- [ ] Shared Library 支持 `filespace_template` 的 typed payload validation、publish、install、source-status。
- [ ] FilespaceTemplate 发布 / 安装支持 text 与 binary 文件，binary 文件 roundtrip 后 MIME、大小与字节内容保持一致。
- [ ] Marketplace 安装 FilespaceTemplate 后生成 Project Filespace，不自动挂载到 Agent runtime。
- [ ] Project preview、Story preview、Session runtime 三类 VFS surface 能解释 Filespace mount 的 owner / purpose / file count / edit capabilities。
- [ ] 后端 migration、domain service、API route、VFS construction、Shared Library mapper 均有覆盖核心路径的测试。
- [ ] 前端 typecheck 与 Filespace / Agent capability 关键 UI 测试通过。

## Out Of Scope

- 不迁移 Story 级 inline VFS 为 Project asset。
- 不把 workspace `main` mount、lifecycle mount、canvas mount、skill-assets mount 统一改造成 Filespace。
- 不把 Filespace 安装后自动挂载给所有 Agent。
- 不做跨 Project 实时同步；Marketplace 更新仍遵循手动重装/覆盖语义。
- 不引入兼容旧字段的长期运行分支。

## Open Questions

- 无。
