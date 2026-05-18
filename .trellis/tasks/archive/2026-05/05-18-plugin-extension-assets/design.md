# Plugin Extension Asset 化 — Design

## Layering

```text
Native Host Plugin
  -> 启动期声明 plugin embedded seeds
  -> Shared Library seed/upsert
  -> Marketplace 展示
  -> 用户安装到 Project
  -> ProjectExtensionInstallation
  -> session construction metadata projection
```

宿主级能力和运行时资产保持分层：

- `AgentDashPlugin` 负责管理员级扩展、启动期贡献和高权限 provider。
- `LibraryAsset` 负责可发现、可安装、可版本追踪的配置资产。
- `ProjectExtensionInstallation` 负责 Project 内启用状态和运行时元数据投影。
- Shared Library asset 本身不直接影响会话；只有用户安装后的 Project extension installation 才会进入 session construction。

## Plugin Embedded Assets

在 plugin API 增加启动期声明能力：

```rust
pub trait AgentDashPlugin {
    fn library_asset_seeds(&self) -> Vec<PluginLibraryAssetSeed> {
        vec![]
    }
}
```

`PluginLibraryAssetSeed` 字段建议：

- plugin_name
- asset_type
- key
- display_name
- description
- version
- payload
- payload_digest 可由宿主统一计算，避免 plugin 自算漂移

宿主收集后写入 Shared Library：

- `scope = system` 或 `builtin`，第一版推荐 `system`，表示来自部署环境能力。
- `source = plugin_embedded`
- `source_ref = plugin:{plugin_name}:{asset_type}:{key}`

需要新增 `LibraryAssetSource::PluginEmbedded`，同步 DB check constraint、DTO、前端类型和展示。

冲突策略沿用 plugin API 的 fail-fast 风格：

- 同一 plugin 内重复 key -> 启动/seed 失败。
- 与现有 builtin/system asset identity 冲突 -> seed 失败，要求 plugin 改 key。
- 不隐式覆盖其它来源资产。

## Extension Template Payload

新增：

```text
LibraryAssetType::ExtensionTemplate
```

payload 草案：

```jsonc
{
  "manifest_version": "1",
  "extension_id": "gitlab-review",
  "commands": [
    {
      "name": "gitlab-review:prepare",
      "description": "准备 GitLab review 上下文",
      "handler": {
        "kind": "inject_message",
        "content": "请基于当前 MR 准备 review。"
      }
    }
  ],
  "flags": [
    {
      "name": "gitlab-review.verbose",
      "type": "bool",
      "default": false,
      "description": "输出更详细诊断"
    }
  ],
  "message_renderers": [
    {
      "custom_type": "gitlab-review.summary",
      "renderer": { "kind": "json_card" }
    }
  ],
  "capability_directives": [],
  "asset_refs": []
}
```

第一版 typed validator 至少校验：

- `extension_id` 非空且稳定。
- command name 不含前导 `/`，命名空间建议以 extension id 开头。
- flag name 非空，类型只支持 bool/string。
- handler kind 只允许安全的声明式 handler，例如 `inject_message`。
- message renderer 只允许 schema-driven renderer kind。

## Project Installation

新增表：

```text
project_extension_installations
```

字段：

- `id`
- `project_id`
- `extension_key`
- `display_name`
- `enabled`
- `config JSONB`
- `manifest JSONB`
- `library_asset_id`
- `source_ref`
- `source_version`
- `source_digest`
- `installed_at`
- `created_at`
- `updated_at`

安装 `extension_template` 时：

1. 读取并校验 `LibraryAssetPayload::ExtensionTemplate`。
2. 生成或覆盖 Project installation。
3. 记录 `InstalledAssetSource`。
4. 默认 `enabled = true`。

source-status 扩展：

- 现有 `ProjectAssetSourceStatus` 增加 `extension_installations` 数组。
- Marketplace install summary 聚合包含 extension installation。

## Runtime Metadata Projection

新 session construction 读取 Project enabled extension installations，生成只读元数据投影：

- slash command definitions
- runtime flag definitions
- message renderer declarations

这个 projection 只服务 `04-12-plugin-extension-api` 后续实现，让 command registry、flag store、renderer registry 有稳定输入源。它不等于 Marketplace asset 直接影响会话，也不在本任务内执行 command、修改 prompt、注册前端 `/` 菜单或写入 hook flag state。

后续独立接线：

- Slash command registry：从 projection 读取 command，并暴露给前端 `/` 菜单。
- Command execution：第一版可只支持 `inject_message` handler；`trigger_hook` 后置。
- Runtime flag store：从 projection 初始化 flag defaults，并暴露给 Hook/Rhai 读取。
- Renderer registry：从 projection 读取 schema-driven renderer declaration；不加载动态前端代码。

## UI

Marketplace：

- 新增 Extension 类型 filter chip。
- Card / drawer 展示 commands、flags、renderers 摘要。
- 安装按钮沿用现有 install 行为。

Project Assets：

- 可新增 Extension 子类目或先在 Marketplace source status 中展示。
- 最小可用路径需要用户能启用/禁用已安装 extension。

## Migration

需要新增 migration：

- `library_assets.asset_type` check 增加 `extension_template`。
- `library_assets.source` check 增加 `plugin_embedded`。
- 新增 `project_extension_installations` 表和唯一索引：
  - `project_id + extension_key`

## Tests

后端：

- plugin seed collection and conflict。
- `plugin_embedded` source parse/serialize/persist。
- extension template payload validation。
- install extension template to Project。
- source-status includes extension installations。
- session construction reads enabled installations。

前端：

- shared-library type mapper accepts extension_template/plugin_embedded。
- Marketplace drawer renders extension summary。
- enable/disable interaction if implemented in this task.
