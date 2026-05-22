# Builtin 资产版本维护与升级治理设计

## 背景

Shared Library 已经承载公共资产、Marketplace 安装、Project installed source 与 source-status 查询。当前缺口集中在 builtin / plugin_embedded seed 的版本事实：builtin seeds 共享单一 `BUILTIN_VERSION`，plugin embedded seeds 虽声明 version，但缺少 payload digest 与 version 关系的自动化治理。结果是 payload 变化可能被普通 `update_available` 吞掉。

本设计把版本治理收敛到 Shared Library seed 层：资产版本由 manifest / typed registry 显式声明，digest 由 canonical payload 自动计算，Project installed source 保存安装时快照，source-status 负责比较 installed snapshot 与当前 LibraryAsset。

## 核心原则

- 资产版本粒度是 `source_ref + asset_type + key`，不是插件包版本，也不是全局 builtin 版本。
- `version` 是维护者声明的升级事实；`payload_digest` 是系统计算出的内容事实。
- Project 资源是可编辑副本，不随 builtin/plugin_embedded source 静默变化。
- 覆盖更新是一组 Project 资源内容与 installed source 元数据的事务性替换。
- 版本维护错误要在开发、测试和启动 seed 阶段 fail-fast，不进入用户可见 UI 状态。

## 数据模型

### Seed Identity

Builtin:

```text
scope = builtin
source = builtin
source_ref = builtin:{asset_type}:{key}
identity = asset_type + scope + owner_id(null) + key
```

Plugin embedded:

```text
scope = system
source = plugin_embedded
source_ref = plugin:{plugin_name}:{asset_type}:{key}
identity = asset_type + scope + owner_id(null) + key
```

Builtin 当前 `source_ref = key` 应迁移为稳定结构化 source_ref，原因是版本错误诊断、snapshot 输出和审计需要同一套可读身份。

### Version Manifest

推荐新增 typed registry，例如 `crates/agentdash-application/src/shared_library/asset_versions.rs`：

```rust
pub struct SeedAssetVersion {
    pub asset_type: LibraryAssetType,
    pub key: &'static str,
    pub version: &'static str,
}
```

`builtin_library_seeds()` 构造 payload 后，通过 `asset_type + key` 查询 version。manifest 中缺失版本时直接返回 `DomainError::InvalidConfig`。这样新增 builtin asset 必须同时声明版本，且 payload digest 继续由 `seed_digest(&payload)` 自动计算。

Plugin embedded 继续使用 `PluginLibraryAssetSeed.version`，宿主只补齐 `source_ref`、计算 digest、校验 payload。后续如果插件 API 暴露 plugin package version，应作为审计字段进入 seed 日志或资产 metadata，不参与 source-status 判断。

## Digest Snapshot 检查

新增测试生成 seed snapshot：

```json
{
  "source_ref": "builtin:workflow_template:builtin.freeform_agent",
  "asset_type": "workflow_template",
  "key": "builtin.freeform_agent",
  "version": "1.1.0",
  "payload_digest": "sha256:..."
}
```

检查规则：

- snapshot 中不存在的新 seed 需要显式加入 snapshot。
- digest 变化且 version 未提升时失败。
- version 提升但 digest 未变化时失败或输出明确诊断；本任务按失败处理，保证版本只表达实际资产变化。
- 删除 seed 时，startup repair 应将旧 LibraryAsset 标记 deprecated，而不是让 installed source 断链。

Snapshot 覆盖 builtin seeds 与 first-party plugin embedded seeds。第三方插件运行时 seed 由宿主在启动期做同类校验：同一 identity/source_ref 的既有 LibraryAsset 如果 digest 变化而 version 未提升，应 fail-fast。

## Source Status

`SharedLibrarySourceStatus` 保持用户可操作三态：

```text
up_to_date
update_available
source_missing
```

比较规则：

| installed vs current | status |
| --- | --- |
| current missing / deprecated | `source_missing` |
| current digest == installed digest | `up_to_date` |
| current digest != installed digest 且 current version > installed version | `update_available` |
| current digest != installed digest 且 current version <= installed version | seed/startup invariant violation，服务启动或 seed repair 失败 |

版本比较应使用 semver 解析；解析失败视为 seed 维护错误并在 seed/startup 阶段失败。当前内置版本已经形如 `1.0.0` / `0.1.0`，可以直接收敛为 semver。

DTO 保留现有 `current_source_version`、`current_source_digest`。不新增维护错误状态或 diagnostic 字段，因为这类错误不属于用户可以修复的 Marketplace 状态。

前端 Marketplace 聚合优先级保持：

```text
source_missing > update_available > up_to_date
```

若 seed/startup invariant 被破坏，后端不应完成启动或 seed repair，前端不会收到这种状态。

## Seed Upsert 与 Repair

Builtin seed upsert：

- seed 构造读取 version manifest；
- 计算 digest；
- validate typed payload；
- upsert 保留既有 `LibraryAsset.id` 与 `created_at`；
- registry 中消失的 builtin asset 标记 deprecated。

Plugin embedded seed upsert：

- 继续检测同一 `asset_type + scope + key` 被不同 plugin/source 占用并 fail-fast；
- 同一 plugin/source_ref 可幂等更新；
- 若既有记录 digest 变化但 version 未提升，启动期 fail-fast；
- 若既有记录 version 无法按 semver 解析，启动期 fail-fast；
- 若 version 提升且 digest 变化，更新 LibraryAsset 并保留 id/created_at。

Startup repair / migration：

- 统一 builtin `source_ref` 到 `builtin:{asset_type}:{key}`；
- 修正 LibraryAsset `payload_digest` 到 canonical digest；
- 对能匹配 LibraryAsset 的 Project installed source 补齐当前 source_ref/version/digest；
- 对旧 builtin bootstrap Project 资源保留为已安装副本。

## 安装与覆盖更新

安装路径继续从 `LibraryAsset` 构造 `InstalledAssetSource`。覆盖更新时每种 Project 资源 mapper 必须在同一应用事务中完成：

- Project 资源内容替换为 LibraryAsset payload 派生内容；
- Project 资源业务 version 按既有规则推进；
- installed source 替换为 LibraryAsset 当前 version/digest/source_ref；
- 失败时内容与 installed source 都不提交。

Workflow template 是 bundle 类型，覆盖更新必须同时处理 workflow definitions 与 activity lifecycle definition，保证同源 bundle 的所有 Project 资源 source metadata 一致。

## 前端边界

前端边界保持用户态展示：

- `SharedLibrarySourceStatus` 继续是 `up_to_date | update_available | source_missing`；
- mapper 可以继续拒绝未知状态或按现有规则处理，但不为维护错误新增用户态 enum；
- Marketplace 卡片、详情抽屉和 install summary 不展示平台维护错误；
- 前端测试只覆盖三态聚合与正常安装/更新入口。

## Spec 更新点

- `.trellis/spec/backend/shared-library.md`：记录 version manifest、digest snapshot、启动期 fail-fast 的设计理由。
- `.trellis/spec/cross-layer/shared-library-contract.md`：确认 source-status 继续只表达用户可操作三态，并说明维护错误由后端启动不变量拦截。
- `.trellis/spec/backend/capability/plugin-api.md`：记录 plugin embedded asset version 与 plugin package version 的分工。
