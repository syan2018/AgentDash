# Extension 资产模型与 Assets 页面收敛

## Goal

将 Extension 管理收敛回 AgentDash 标准三层资产模型：

- Marketplace / Shared Library 存放可发现、可安装、可发布的 `ExtensionTemplate`。
- Project 内只以 `ProjectExtensionInstallation` 作为 Extension 实例事实源。
- Extension package artifact 是安装和运行所需的包工件，不作为用户面对的第二套资源市场或归档库。

完成后，用户应能在标准 Assets/Marketplace 流程中安装、发布、更新、卸载和诊断 Extension；从 Marketplace 安装的 packaged Extension 必须可运行，从本地包导入的 Extension 必须表现为 Project 资产实例，而不是落入独立“归档库”心智。

## Context And Confirmed Facts

- `.trellis/spec/cross-layer/shared-library-contract.md` 定义 Shared Library / Marketplace / Project Asset 三层模型，并明确 Project runtime 只读取安装后的 Project 资源。
- `ProjectExtensionInstallation` 已经是 Project 内 Extension 实例实体，并同时支持 `installed_source` 与 `package_artifact`。
- `extension_package_artifacts` 当前是 project-scoped metadata + archive storage，packaged Extension 安装会把 `package_artifact` 记录到 installation。
- Marketplace 已经展示并安装 `extension_template`，source-status 也包含 `extension_installations`。
- 当前 Assets Extension 类目直接消费 `extension-runtime` projection 和 `extension-artifacts` list，UI 分成“已安装”和“归档库”，没有走 Project asset/source-status 管理模型。
- `PublishLibraryAssetKind` 与后端 publish use case 目前没有 `extension_installation`，Project Extension 不能按标准发布到 Marketplace。
- Local runtime / gateway 对可执行 host、protocol channel、webview/canvas panel 依赖 package artifact；缺少 artifact 的 marketplace-installed Extension 可能显示为已安装但无法完整运行。

## Planning Decisions

- Artifact ownership 推荐采用统一 `owner_kind = project | library_asset` 模型。这个模型把 package artifact 的 digest、manifest、storage、runtime access validation 保持在同一个事实源里，只把所有权作为维度表达。
- 本任务按单个完整 task 推进，通过 Phase 1-6 分阶段实现和验证；不预先拆 child tasks。各阶段必须能独立通过窄验证，再进入下一阶段。

## Problem Statement

当前 Extension 已经同时存在 `ExtensionTemplate`、`ProjectExtensionInstallation` 和 package artifact 三个概念，但前端管理面把 package artifact 提升成“归档库”，并把 runtime projection 当成管理事实源。这样会造成：

- Marketplace 安装、Project 安装、本地包上传之间的语义不一致。
- Extension 无法使用标准发布/更新/source-status 流程。
- 用户看到两个类似“库”的入口：资源市场和 Extension 归档库。
- 可执行 Extension 的 package artifact 完整性无法在 Marketplace 安装入口被强约束。

## Requirements

### R1 Asset Model

- `ProjectExtensionInstallation` 是 Project Extension 实例的唯一管理事实源。
- `ExtensionPackageArtifact` 是 Extension 包工件，可归属于 Project 本地导入或 LibraryAsset 发布物；它不作为用户面对的独立资产类型。
- `ExtensionTemplate` 表达 Marketplace/Shared Library 中的 Extension 模板及 manifest 声明；若模板包含可执行/可渲染包内容，必须能关联可校验 package artifact。
- `installed_source` 继续表达 Marketplace/Shared Library 来源版本；`package_artifact` 继续表达运行所需包工件版本与 digest。二者可以同时存在。

### R2 Install Semantics

- 从 Marketplace 安装 `extension_template` 时，后端按模板生成或覆盖 `ProjectExtensionInstallation`。
- 对包含 host runtime action、protocol channel、workspace tab、webview/canvas panel 或 bundle 的 ExtensionTemplate，安装时必须绑定可用 package artifact；缺失 artifact 时安装失败并返回明确错误。
- 对 command/flag/message-renderer 等不需要包工件执行的声明型 ExtensionTemplate，可以保持 manifest-only installation，但 API/前端必须能展示它是声明型资产。
- 从本地 `.agentdash-extension.tgz` 导入时，后端校验 archive digest、manifest digest、package metadata、bundle digest 与 panel entry，并创建 Project extension installation。上传包不在 UI 中形成独立“归档库”。
- 卸载只移除 Project extension installation；包工件按所有权和引用关系由 artifact 存储规则维护。

### R3 Publish Semantics

- Project Extension installation 可以作为 `extension_installation` 发布到 Shared Library。
- 发布请求仍只传 Project 资源身份、版本、key、display_name、description 和 overwrite；前端不得传 raw manifest/payload。
- 后端从 Project installation 读取 manifest 与 package artifact，生成 `extension_template` LibraryAsset，并为该 LibraryAsset 保存或关联 package artifact。
- 发布后的 ExtensionTemplate 被另一个 Project 安装时，必须能得到 `installed_source + package_artifact` 的 Project installation。

### R4 Management API And Source Status

- 新增或重构 Project Extension 管理 API，返回 Project installation 列表、来源状态、package artifact 摘要、能力摘要和声明型/packaged 类型。
- Assets 管理页不得使用 `extension-runtime` projection 作为管理事实源；runtime projection 继续服务 Workspace/runtime tab catalog。
- Shared Library source-status 继续包含 `extension_installations`，并在前端 Marketplace 与 Project Extension 类目中统一展示 `up_to_date / update_available / source_missing`。
- Contract crate 和生成的 TypeScript DTO 必须与新 API 对齐。

### R5 Frontend Assets And Marketplace UX

- Assets 页 Extension 类目收敛为标准 Project asset 管理视图：列表卡片、来源徽标、发布状态、source-status、详情抽屉、卸载、发布/更新发布、从本地包安装。
- 移除顶层“归档库”section；保留“从本地包安装”作为导入/安装动作。
- Marketplace Extension 详情页展示 package metadata、runtime actions、protocol channels、workspace tabs、permissions、bundle digest、artifact 完整性和安装状态。
- Marketplace Extension 安装/更新与其它资产共用安装状态心智。

### R6 Runtime Integrity

- WorkspacePanel、runtime action、protocol channel、webview/canvas panel 继续只从 Project enabled installation 派生。
- local runtime 下载 artifact 时必须继续校验 archive digest。
- 对缺 artifact 的 executable Extension，runtime gateway 应在安装或调用前给出明确诊断，避免“已安装但运行时才神秘失败”。

### R7 Migration And Cleanup

- 新增数据库 migration，把现有 project-scoped extension package artifacts 迁入新的 artifact ownership 模型或等价结构。
- 迁移现有 Project installation 中的 `installed_source` / `package_artifact` 字段，确保 source-status 和 runtime projection 仍能重建。
- 清理前端旧 service/UI/test 中的“归档库”语义，保留必要的 package import/download 能力。
- 更新 cross-layer/frontend/backend 相关 spec，记录新的资产模型原因和运行约束。

## Acceptance Criteria

- [ ] AC1: Marketplace 安装一个带 runtime action / workspace tab 的 packaged Extension 后，Project installation 同时包含 `installed_source` 与 `package_artifact`，WorkspacePanel 能出现对应 tab，runtime action 能通过本机 host 执行。
- [x] AC2: Marketplace 安装一个声明了 executable surface 但缺少 package artifact 的 ExtensionTemplate 时，后端拒绝安装并返回明确错误；前端 Marketplace 不显示为可正常安装。
- [x] AC3: 从 Assets Extension 类目执行“从本地包安装”后，页面只出现一个 Project Extension 资产实例；页面不再出现顶层“归档库”section。
- [ ] AC4: Project Extension installation 可以发布为 `extension_template` 到 Marketplace；另一个 Project 从 Marketplace 安装后可运行，并在 source-status 中显示 `up_to_date`。
- [ ] AC5: 已发布 Extension 更新版本后，安装方 Project 的 Extension 类目和 Marketplace 卡片均显示 `update_available`，用户手动更新后 source-status 回到 `up_to_date`。
- [x] AC6: Extension 类目使用标准 Assets UI 心智：来源徽标、发布徽标、详情抽屉、CardMenu/ConfirmDialog/Notice 等既有 primitive 或同等项目内模式；不再维护独立来源 badge/归档库列表。
- [x] AC7: Local runtime 下载和激活 package artifact 仍校验 archive digest；artifact ownership 重构不降低下载鉴权与 project/backend access 校验。
- [x] AC8: 数据库 migration 可在当前开发库上运行；迁移后现有 Project extension installations 和 package artifacts 能被新 repository/API 正确读取。
- [x] AC9: `cargo test` 覆盖 domain/application/API 关键路径，`pnpm --filter app-web run typecheck` 和相关 vitest 通过，contracts 生成与检查通过。
- [x] AC10: 相关 spec 更新说明 ExtensionTemplate、ProjectExtensionInstallation、ExtensionPackageArtifact 三者关系以及 Marketplace install/publish/source-status 的事实源。

## Scope Boundaries

- 本任务不重新设计 Extension SDK authoring API，只调整安装、发布、管理和 artifact ownership 相关的宿主侧模型。
- 本任务不引入远端 SaaS marketplace 同步协议；Shared Library 仍是当前后端事实源。
- 本任务不重做 permission grant 产品体验，但必须保留/展示 manifest permission 摘要和运行时 permission validation。
- 本任务不要求对已打开的 Workspace extension tab 做强制关闭；安装列表刷新后新建 tab catalog 必须正确。

## Notes

- 当前任务为复杂跨层重构，进入实现前必须保留 `design.md` 与 `implement.md`，并由用户确认规划。
- 仓库当前存在其它 Extension Host 相关未提交改动；实现时必须先确认工作区状态并避免覆盖他人改动。
