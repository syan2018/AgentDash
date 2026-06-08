# Research: Projection 源类型 (ExtensionRuntimeProjection 及子投影)

- **Query**: ExtensionRuntimeProjection 及全部子投影字段定义，所在 crate，serde/ts 导出状态
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### 所在 crate

源 projection 类型全部定义在 **application** crate，文件
`crates/agentdash-application/src/extension_runtime.rs`（`pub use` 经 `crates/agentdash-application/src/lib.rs`）。

- **没有 serde**：所有 projection struct 只 derive `Debug, Clone, PartialEq`（部分加 `Eq`），**不带 `Serialize/Deserialize`**，也**不带 `ts_rs::TS`**。
- 对外 TS/JSON 暴露走 **contracts crate 的镜像 `*Response` 类型**（见 `07-contracts-and-codegen.md`），由 API 层 mapper 手动转换。

### 完整字段定义（`extension_runtime.rs`）

顶层聚合（L17-29）：

```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtensionRuntimeProjection {
    pub installations: Vec<ExtensionInstallationProjection>,
    pub commands: Vec<ExtensionCommandProjection>,
    pub flags: Vec<ExtensionFlagProjection>,
    pub message_renderers: Vec<ExtensionMessageRendererProjection>,
    pub runtime_actions: Vec<ExtensionRuntimeActionProjection>,
    pub protocol_channels: Vec<ExtensionProtocolChannelProjection>,
    pub extension_dependencies: Vec<ExtensionDependencyProjection>,
    pub workspace_tabs: Vec<ExtensionWorkspaceTabProjection>,
    pub permissions: Vec<ExtensionPermissionProjection>,
    pub bundles: Vec<ExtensionBundleProjection>,
}
```

子投影逐个（file:line）：

| 类型 | line | 关键字段 |
|---|---|---|
| `ExtensionInstallationProjection` | L31-39 | `installation_id: Uuid`, `extension_key: String`, `extension_id: String`, `display_name: String`, `installed_source: Option<InstalledAssetSource>`, `package_artifact: Option<ExtensionPackageArtifactRef>` |
| `ExtensionCommandProjection` | L41-48 | `extension_key`, `extension_id`, `name`, `description`, `handler: ExtensionCommandHandler` |
| `ExtensionFlagProjection` | L50-58 | `extension_key`, `extension_id`, `name`, `flag_type: ExtensionFlagType`, `default: serde_json::Value`, `description` |
| `ExtensionMessageRendererProjection` | L60-66 | `extension_key`, `extension_id`, `custom_type`, `renderer: ExtensionRendererDeclaration` |
| `ExtensionRuntimeActionProjection` | L68-78 | `extension_key`, `extension_id`, `action_key: String`, `kind: ExtensionRuntimeActionKind`, `description`, `input_schema: serde_json::Value`, `output_schema: serde_json::Value`, `permissions: Vec<String>` |
| `ExtensionProtocolChannelProjection` | L80-88 | `extension_key`, `extension_id`, `channel_key`, `version`, `description`, `methods: Vec<ExtensionProtocolChannelMethodProjection>` |
| `ExtensionProtocolChannelMethodProjection` | L90-97 | `name`, `description`, `input_schema`, `output_schema`, `permissions: Vec<String>` |
| `ExtensionDependencyProjection` | L99-104 | `extension_key`, `extension_id`, `dependency: ExtensionDependencyDeclaration` |
| `ExtensionWorkspaceTabProjection` | L106-114 | `extension_key`, `extension_id`, `type_id: String`, `label: String`, `uri_scheme: String`, `renderer: ExtensionWorkspaceTabRendererDeclaration` |
| `ExtensionPermissionProjection` | L116-121 | `extension_key`, `extension_id`, `permission: ExtensionPermissionDeclaration` |
| `ExtensionBundleProjection` | L123-130 | `extension_key`, `extension_id`, `kind: ExtensionBundleKind`, `entry: String`, `digest: String` |

子类型中的 `ExtensionCommandHandler / ExtensionFlagType / ExtensionRendererDeclaration /
ExtensionRuntimeActionKind / ExtensionDependencyDeclaration /
ExtensionWorkspaceTabRendererDeclaration / ExtensionPermissionDeclaration /
ExtensionBundleKind / InstalledAssetSource` 都来自
`agentdash_domain::shared_library`（import 见 L6-12）；`ExtensionPackageArtifactRef`
来自 `agentdash_domain::extension_package`（L5）。

### 构造入口

`pub fn extension_runtime_projection_from_installations(installations: Vec<ProjectExtensionInstallation>) -> Result<ExtensionRuntimeProjection, DomainError>`
（L132-290）。从 manifest 展平，并对 `runtime action key / protocol channel key /
workspace tab type_id / uri_scheme` 做全局唯一性校验（`claim_unique_extension_runtime_key`, L368-384）。
`ProjectExtensionInstallationRepository::list_enabled_by_project` 是 enabled installation 的数据源
(见测试 fake L609-623，trait import L11)。

## Caveats / Not Found

- Projection 在 application 层、无 serde；若要进入 `AgentFrame` 的 JSON surface
  字段或写入 contract，需要新增 serde 或经由 contracts `*Response` 镜像类型转换。
