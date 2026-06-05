# Backend Shared Library

本文档只记录 Shared Library 后端专属基线：seed、validator、安装事务和 plugin embedded 资产。跨层权威契约见 [Shared Library Contract](../cross-layer/shared-library-contract.md)。

## Backend Role

后端负责把 `LibraryAsset` 的灵活 JSON payload 收束为类型化领域对象，并在安装、发布、更新、seed 阶段维护来源、版本和 digest 不变量。

## Invariants

- `LibraryAsset.payload` 只能在 Shared Library 边界保持 JSONB 灵活性。
- 每个 `asset_type` 必须有类型化 mapper / validator。
- 运行路径不得直接消费未校验的 `payload`，必须先安装成 Project 资源或转换成类型化领域对象。
- Project 资源不会因 builtin seed 更新而静默变化。
- `payload_digest` 由 canonical JSON sha256 规则自动计算，不手写。
- payload digest 变化时 version 必须提升；version 提升时 payload digest 也必须变化。
- version/digest 不变量破坏属于平台维护错误，必须 seed/startup fail-fast。

## LibraryAsset Backend Baseline

`LibraryAsset` 使用单表 JSONB payload：

- `asset_type`
- `scope`
- `owner_id`
- `key`
- `display_name`
- `description`
- `version`
- `source`
- `source_ref`
- `payload_digest`
- `deprecated`
- `payload`

唯一身份：`asset_type + scope + owner_id + key`。

## BuiltinSeedRegistry

Builtin 资产通过统一 seed registry 物化到 Shared Library，不在各资源模块中单独 bootstrap。

Registry 负责：

- 收集内置 `AgentTemplate` / `McpServerTemplate` / `WorkflowTemplate` / `SkillTemplate` 等定义。
- 为每个 seed 提供稳定 `builtin_key`、`version`、`payload_digest`。
- 幂等 upsert 到 `LibraryAsset`。
- 对 registry 中删除的 builtin 默认标记 `deprecated`，避免已安装 Project 资源来源断链。

`source_ref` 使用 `builtin:{asset_type}:{key}`。

## InstalledAssetSource

安装到 Project 后的资源必须记录来源：

- `library_asset_id`
- `source_ref`
- `source_version`
- `source_digest`
- `installed_at`

Project 资源保留 `InstalledAssetSource`，用于审计、重装和版本提示。

## Project Install Semantics

- 从 Marketplace 安装默认创建可编辑 Project 副本。
- Project 运行时只读取 Project 资源，不直接依赖 Shared Library。
- `ProjectAgent` 属于 Project 资源，安装 `AgentTemplate` 时必须创建 ProjectAgent，并写入 `InstalledAssetSource`。
- Workflow/Lifecycle bundle 安装和更新必须在一个数据库事务中提交 workflow definitions 与 activity lifecycle definition。
- 失败的 workflow template update 必须保持 project resources 与 installed source metadata 不变。

## Project Publish Semantics

- 发布入口从 Project 资源出发：`POST /api/projects/{project_id}/shared-library/publish`。
- 发布请求只提交资源类型、Project 资源 id、资产元数据和覆盖策略。
- 后端重新读取 Project 资源权威状态，并通过类型化 mapper 生成对应 `*Template` payload。
- 发布身份沿用 `asset_type + scope + owner_id + key`。
- `overwrite=false` 时同身份存在返回冲突。
- 覆盖发布必须保留原 `LibraryAsset.id` 与 `created_at`，更新 payload、version、digest 与 metadata。
- MCP Preset 发布必须拒绝 credential、header、env、本机路径、localhost/private network URL 等连接材料。

## Integration Embedded Assets

Native integration 可在启动期通过 `AgentDashIntegration::library_asset_seeds()` 声明内嵌 Shared Library assets。

Contract:

- integration 只声明 `IntegrationLibraryAssetSeed`，不直接写数据库，也不修改 Project 运行配置。
- 宿主统一计算 digest、设置 `scope=system`、`source=integration_embedded` 和 `source_ref=integration:{integration_name}:{asset_type}:{key}`。
- seed payload 必须通过 Shared Library typed validator。
- 同一 `asset_type + scope + key` 被不同 integration 或不同 source 占用时启动期 fail-fast。
- 同一 integration 的同一 seed 可幂等更新，保留原 `LibraryAsset.id` 与 `created_at`。
- integration seed 的 `version` 是资产版本，不等同于 integration 包版本。
