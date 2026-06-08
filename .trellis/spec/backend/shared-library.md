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
- `McpServerTemplate` 安装可携带 `install_options.mcp_server_template.parameters`，后端用公共 `transport_template` 与 `parameter_schema` 解析出具体 Project MCP Preset，原因是用户连接输入只属于 Project 安装事务。
- Workflow/Lifecycle bundle 安装和更新必须在一个数据库事务中提交 workflow definitions 与 activity lifecycle definition。
- 失败的 workflow template update 必须保持 project resources 与 installed source metadata 不变。

## Project Publish Semantics

- 发布入口从 Project 资源出发：`POST /api/projects/{project_id}/shared-library/publish`。
- 发布请求只提交资源类型、Project 资源 id、资产元数据和覆盖策略。
- 后端重新读取 Project 资源权威状态，并通过类型化 mapper 生成对应 `*Template` payload。
- 发布身份沿用 `asset_type + scope + owner_id + key`。
- `overwrite=false` 时同身份存在返回冲突。
- 覆盖发布必须保留原 `LibraryAsset.id` 与 `created_at`，更新 payload、version、digest 与 metadata。
- MCP Preset 发布生成 `transport_template` 公共模板；HTTP/SSE preset 可以发布为无 header 的 URL template，stdio preset 保持 Project 资源语义，原因是 stdio 涉及本机进程、路径和 env 治理，不能成为公共市场模板。
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

## External Marketplace Import

外部 Marketplace provider 只提供发现、详情和 fetched payload。后端导入入口负责把 fetched payload 收束成平台内 `LibraryAsset`，原因是版本、digest、scope、owner 与后续 Project install 必须继续由 Shared Library 统一维护。

Contract:

- import mode `upsert_library_asset` 写入 `LibraryAsset(source=remote_imported)`。
- `source_ref` 使用 `market:{source_key}:{asset_type}:{external_id}`。
- `payload_digest` 由 canonical JSON sha256 规则自动计算，远端 digest 只保留为 refresh 比较输入。
- fetched asset 的 `source_key`、`external_id`、`asset_type` 必须与请求一致。
- fetched payload 必须通过 `LibraryAsset::new` 与 `LibraryAssetPayload` typed validator。
- `mcp_server_template` fetched payload 必须使用 HTTP/SSE `transport_template`、`parameter_schema` 与 `capabilities`，导入阶段不保存 header/env/credential 值或本机绑定配置。
- 同一 `asset_type + scope + owner_id + key` 下，同一 `remote_imported` source_ref 可幂等更新；其它来源占用同一身份时返回冲突。
- refresh 读取 provider detail 与本地 `remote_imported` LibraryAsset 做 version/digest 比较，不写入 Project 资源。

## Scenario: Skill URL Import Convergence

### 1. Scope / Trigger

`POST /api/projects/{project_id}/skill-assets/import` 是 Project 级 URL Import 入口，但远端来源版本、digest 与审计事实属于 Shared Library。该入口必须把 URL 定位到的 Skill 先物化为 `skill_template` LibraryAsset，再安装为 Project SkillAsset，原因是 catalog import 与 URL import 都是外部来源写入。

### 2. Signatures

HTTP 入口保持：

```text
POST /api/projects/{project_id}/skill-assets/import
{ "url": "<github | clawhub | skills.sh url>" }
```

Application 入口：

```rust
pub async fn import_remote_skill_url_to_project(
    repos: &RepositorySet,
    input: ImportRemoteSkillAssetInput,
    source: &dyn RemoteSkillSource,
) -> Result<SkillAsset, SkillAssetApplicationError>;
```

Materializer 入口：

```rust
pub fn materialize_remote_skill_template(
    input: RemoteSkillTemplateInput,
) -> Result<MaterializedSkillTemplate, SkillAssetApplicationError>;
```

### 3. Contracts

- `RemoteSkillSource::fetch(url)` 负责 GitHub / ClawHub / skills.sh 解析、文件数量限制、单文件限制和总大小限制。
- materializer 负责 content typing、根目录 `SKILL.md` metadata 解析、`validate_skill_files` 和 `SkillTemplatePayload` 生成。
- URL import 的 `source_ref` 使用 `market:skill-url:{source_kind}:{sha256(normalized_url)}`。
- `LibraryAsset` 写入 `asset_type=skill_template`、`scope=user`、`source=remote_imported`、`owner_id=current_user`。
- 写入 `LibraryAsset` 前必须确认 Project 中同 key 与同 `source_ref` 都可覆盖，原因是 Project 冲突不应留下新的 `remote_imported` Shared Library 资产。
- Project SkillAsset 通过 `install_library_asset_to_project(..., overwrite=true)` 创建或更新，并写入 `InstalledAssetSource`。
- Project SkillAsset 的 `source` 保持 Project 本地来源语义；远端来源事实由 `LibraryAsset.source_ref` 与 `InstalledAssetSource` 表达。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| URL 为空、格式非法或 host 不支持 | `BadRequest` |
| 根目录缺少 `SKILL.md` | `BadRequest` |
| `SKILL.md` metadata 非法 | `BadRequest` |
| fetched 文件不能转成 `SkillTemplatePayload` | `BadRequest` |
| 同 Project 中同 key 已由其它来源或用户资产占用 | `Conflict` |
| 同 Project 中同 `source_ref` 已安装到不同 key | `Conflict` |
| 同 user scope 的 LibraryAsset identity 被其它来源占用 | `Conflict` |
| remote provider 内部失败 | `Internal` |

### 5. Behavior Cases

- 首次导入：GitHub URL fetch 成功，materializer 生成 `skill_template`，Shared Library 写入 `remote_imported`，install 创建带 `InstalledAssetSource` 的 Project SkillAsset。
- 同源更新：同一 URL 再次导入，source_ref 相同，LibraryAsset 保留 id 并更新 payload/version/digest，Project SkillAsset 以同 installed source 覆盖。
- 冲突保护：Project 已有同 key 的手写 SkillAsset，或同 `source_ref` 已安装到不同 key 时，导入返回冲突，Shared Library 不写入新的 remote_imported asset。

### 6. Tests Required

- materializer 输出稳定 `source_ref`、`SkillTemplatePayload` 和 digest-derived version。
- 缺少 `SKILL.md`、二进制 payload 或非法 files 返回 `BadRequest`。
- 同 key 非同 source 的 Project SkillAsset 触发 `Conflict`。
- 同 `source_ref` 但 Project key 不一致触发 `Conflict`。
- 同 source_ref 的重复导入更新同一个 `remote_imported` LibraryAsset。
- route 编译测试保持 `{ url } -> SkillAssetResponse` wire 形态。
