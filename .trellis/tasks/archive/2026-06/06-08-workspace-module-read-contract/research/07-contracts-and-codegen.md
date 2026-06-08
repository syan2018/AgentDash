# Research: Contracts / 类型生成 (DTO 声明 → packages/app-web/src/generated)

- **Query**: contracts crate 组织；DTO 声明/导出方式；contracts:check 实际跑什么；范例 DTO 文件
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### contracts crate 组织

`crates/agentdash-contracts/src/lib.rs` 按 domain 分模块（L1-15）：
`companion / core / extension_management / extension_package / extension_runtime /
external_marketplace / llm_provider / mcp_preset / permission / project_agent /
session / settings / shared_library / vfs / workflow`。

### DTO 声明方式

每个 DTO 是带 `ts_rs::TS` 的 serde struct/enum。范例文件
**`crates/agentdash-contracts/src/extension_runtime.rs`**（最贴合本任务）：

```rust
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionRuntimeActionKindResponse { SessionRuntime, Setup }   // L7-12

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtensionInstallationProjectionResponse {                    // L111-119
    pub installation_id: String,   // Uuid 在 contract 里降级为 String
    pub extension_key: String,
    pub extension_id: String,
    pub display_name: String,
    pub installed_source: Option<ExtensionInstalledAssetSourceResponse>,
    pub package_artifact: Option<ExtensionPackageArtifactRefResponse>,
}
```

惯例：
- 类型命名 `*Response` / `*Dto` / `*Request`。
- 内部 domain/application 类型（如 `ExtensionRuntimeProjection`，Uuid，DateTime）**不直接**
  derive TS；contracts 层做镜像类型，Uuid→String、DateTime→rfc3339 String、tagged enum 用
  `#[serde(tag = "kind", rename_all = "snake_case")]`（见 `ExtensionCommandHandlerResponse` L41-45）。

### projection → response 的手写 mapper

`crates/agentdash-api/src/dto/extension_runtime.rs`：
- L1 `use agentdash_application::extension_runtime::ExtensionRuntimeProjection;`
- L9-22 `pub use agentdash_contracts::extension_runtime::{...Response};`
- `pub fn extension_runtime_projection_response(projection: ExtensionRuntimeProjection) -> ExtensionRuntimeProjectionResponse`
  (L24-...)：逐字段 `.into_iter().map(...)` 转换（Uuid→to_string、DateTime→to_rfc3339）。
  这是「application projection → contract response」的标准收口点。

### TS 导出注册

`crates/agentdash-contracts/src/generate_ts.rs`（bin `generate_contracts_ts`）：
- 输出目录 `packages/app-web/src/generated`（L140-141）。
- 用 `emit_domain(dir, "<domain>-contracts.ts", &mut upstream, check, |dir| { export_all::<T>(dir); ... })`。
- extension-runtime block：L452-486，文件名 `"extension-runtime-contracts.ts"`，
  顶层聚合 `export_all::<ExtensionRuntimeProjectionResponse>(dir);`（L482）。
  **新增 DTO 必须在此显式 `export_all::<NewResponse>(dir)` 注册，否则不会生成 TS。**
- `upstream` map 用于去重跨 domain 引用类型。

### contracts:check 实际命令

`package.json`：
- L44 `"contracts:generate": "cargo run -p agentdash-contracts --bin generate_contracts_ts"`
- L45 `"contracts:check": "cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check"`
- L47 `check` 聚合脚本第一步即 `contracts:check`。

`--check` 在 `generate_ts.rs::main` 由 `let check = env::args().any(|a| a == "--check");`
(L139) 控制：check 模式比对生成结果与磁盘是否一致（不写入），用于 CI 守门。

## Caveats / Not Found

- 新增 workspace module contract 应：在 `agentdash-contracts/src/<domain>.rs` 加 `*Response`
  （derive TS + serde），在 `generate_ts.rs` 对应 emit_domain block `export_all::<...>`，
  在 `agentdash-api/src/dto/<domain>.rs` 写 projection→response mapper。
- 未确认 module 应归入哪个既有 domain 文件还是新建 `workspace_module.rs`；design 决定。
