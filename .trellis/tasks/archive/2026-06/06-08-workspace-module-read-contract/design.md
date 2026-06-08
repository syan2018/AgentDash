# Design · Workspace Module 读路径与单一契约

> Parent design: `.trellis/tasks/06-08-workspace-module-registry/design.md`。研究依据：本任务 `research/01..07`。

## 1. 范围

只读路径：定义单一 Workspace Module 契约 DTO，聚合 enabled extension + visible canvas（+ builtin 预留）为同一种 module descriptor，经 `workspace_module_list` / `workspace_module_describe` 暴露给 Agent；预留可见性裁切并走 Capability 通道；收口 `extension_runtime` 死字段。无 invoke / present（Child 2）、无管理 UI（Child 3）。

## 2. 契约 DTO（crates/agentdash-contracts，serde + ts_rs）

新增 `contracts/src/workspace_module.rs`，全部 `#[derive(Serialize, Deserialize, TS)]`，Uuid → String：

```text
WorkspaceModuleKind     = "extension" | "canvas" | "builtin"
WorkspaceModuleStatus   = "ready" | "unavailable"  (+ reason: Option<String>)

WorkspaceModuleSummary {            // list 返回，无完整 schema
  module_id: String                 // 稳定 id，见 §4
  kind: WorkspaceModuleKind
  title: String
  description: String
  source: String                    // extension_key / canvas mount / builtin key
  ui_summary: Option<String>        // 有几个 UI entry 的简述
  operation_summary: Vec<String>    // operation_key 列表（仅 key）
  status: WorkspaceModuleStatus
}

WorkspaceModuleUiEntry {
  view_key: String
  renderer_kind: String             // "webview" | "canvas" | "panel"
  uri_scheme: Option<String>
  title: String
}

WorkspaceModuleOperation {
  operation_key: String
  origin: String                    // "runtime_action" | "protocol_channel" | "canvas" | "builtin"
  description: String
  input_schema: Option<serde_json::Value>
  output_schema: Option<serde_json::Value>
  permission_summary: Vec<String>
}

WorkspaceModuleDescriptor {         // describe 返回
  summary: WorkspaceModuleSummary
  ui_entries: Vec<WorkspaceModuleUiEntry>
  operations: Vec<WorkspaceModuleOperation>
  runtime_backing: Option<String>   // 引用底层 runtime surface（如 extension_runtime / canvas mount）
}
```

`channel-as-operation`（D3）：`protocol_channels[].methods[]` 映射为该 provider extension module 的 `WorkspaceModuleOperation{ origin: "protocol_channel" }`，与 `runtime_actions` 同列，**不单独成 module**。

三处同步（research/07）：`contracts/src/workspace_module.rs` 定义 → `generate_ts.rs` 加 `export_all::<...>` → 若需要 API 返回再加 `api/src/dto`。本 child 的 module 数据只供 Agent 工具，**不新开 HTTP 路由**，故 api/dto 可暂缓；ts_rs 导出仍做，供 Child 3 UI 复用。

## 3. 聚合（application）

新增 `crates/agentdash-application/src/workspace_module/mod.rs`：

```text
build_workspace_modules(
  ext: &ExtensionRuntimeProjection,   // 复用 extension_runtime_projection_from_installations
  canvases: &[Canvas],
  // builtin: 预留，先空
) -> Vec<WorkspaceModuleDescriptor>
```

- extension：每个 installation → 一个 module；其 `runtime_actions` + `protocol_channels.methods` → operations；`workspace_tabs` → ui_entries；`permissions` → permission_summary；`bundles` 缺失 → status=unavailable(reason)。
- canvas：每个 visible canvas → 一个 module（kind=canvas）；entry/files → ui_entry(canvas)；canvas bindings/runtime-invoke → operations（origin=canvas，schema 先给最小）。
- 复用现成函数：`extension_runtime_projection_from_installations`、`list_project_canvases` / `append_visible_canvas_mounts`。

> 注：`ExtensionRuntimeProjection` 子类型当前无 serde（research/01）。聚合层把它们**转换**成上面的 contract DTO，不直接序列化内部投影，避免给内部类型加 serde。

## 4. module_id 约定

- extension：`ext:{extension_key}`（installation 唯一）
- canvas：`canvas:{mount_id}`
- builtin：`builtin:{key}`（预留）

稳定、可读、可被 Child 2 invoke 反解析来源。

## 5. Agent 工具（挂 RelayRuntimeToolProvider）

研究结论：挂同一 provider（已持 repos + project_id_from_context），样板 `ListCanvasesTool`（canvas/tools.rs）。

新增 `crates/agentdash-application/src/workspace_module/tools.rs`：

- `WorkspaceModuleListTool` → `workspace_module_list`，无参（或可选 kind 过滤），返回 `Vec<WorkspaceModuleSummary>`。
- `WorkspaceModuleDescribeTool` → `workspace_module_describe`，参 `module_id`，返回 `WorkspaceModuleDescriptor`；未知 id 返回结构化错误。

二者在 `RelayRuntimeToolProvider::build_tools` 中按新 capability tool flag 门控加入；内部用 `project_id_from_context` 拿 project，调用 repos 现取现算（与 canvas 工具一致），再经 §6 可见性过滤。schema delta 只新增这两个工具本身（验收点）。

## 6. 可见性裁切走 Capability 通道（D4）

- 新增 `CapabilityState` 维度 `WorkspaceModuleVisibility`（spi，模板 `SkillDimension`，research/05）：
  - `mode: "all" | "allowlist"`，`allowed_module_ids: Vec<String>`。默认 `all`。
- launch capability 解析时填充该维度；本 child 默认 `all`（全集 = enabled extension + visible canvas）。
- 工具在返回前按该维度过滤 projection。`ExecutionContext` 直接可取 `capability_state`（research/03），故工具能读到。
- **AgentFrame 预留字段**：`visible_workspace_module_refs_json`，getter/append 完全镜像 `visible_canvas_mount_ids_json`（research/05）。capability 组装时若该字段非空 → 生成 `allowlist`；为空 → `all`。本 child 只落字段 + 组装读取，**编辑入口在 Child 3**。

> 这满足"预留字段 + 走 capability 通道、不形成第二套规则"：唯一进入工具的可见性来源是 capability 维度，AgentFrame 字段只是它的 upstream 输入之一。

## 7. D6 死字段收口 —— 选择删除

研究结论：`FrameLaunchIntent.extension_runtime` 生产唯一写点是 `frame_construction/mod.rs:381` 填 `None`、无读取；`ConstructionProjections.extension_runtime` 仅测试 fixture 读。新读路径由工具现取现算，不依赖该 threading。故**删除**这两个字段及其测试引用，避免与新 projection 并存（parent D6 / 风险条）。

## 8. 不做 / 边界

- 不做 invoke / present（Child 2）。
- 不做管理 UI、不做裁切编辑入口（Child 3）。
- 不新增 HTTP 路由（module 数据本 child 只供 Agent 工具；ts_rs 导出供 UI 复用）。
- builtin module 仅预留 kind 与聚合占位，先不实装具体 builtin。

## 9. 验收（见 prd.md）对应实现点

| 验收 | 落点 |
|---|---|
| list 看到 ext+canvas 摘要 | §3 聚合 + §5 list 工具 |
| describe 返回 operations（channel 同构） | §2 DTO + §3 channel-as-operation |
| schema delta 只增 2 工具 | §5 门控 |
| 不成为新事实源 | §3 现取现算，引用 installation/canvas |
| 死字段收口 | §7 删除 |
| projection 可被 UI 复用 | §2 ts_rs 导出 |
| 类型生成通过 | §2 三处同步 |
