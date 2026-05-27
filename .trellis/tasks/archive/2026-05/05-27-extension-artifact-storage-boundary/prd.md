# Extension Artifact Storage 边界抽离

## Goal

把 extension package artifact 的 filesystem storage helper 从 API route 中抽离，让 archive upload/download、webview asset read 与 Canvas promote 后续能复用同一 application/infrastructure 边界，避免 route 之间互相 import 业务 helper。

## Requirements

- API route 只保留鉴权、DTO、调用 application service 与错误映射。
- `extension_package_artifacts` route 不再直接拥有 `storage_root`、`write_storage_object`、`read_storage_object` 这类 storage helper。
- `extension_runtime` route 不再从另一个 route 模块 import artifact storage helper。
- storage root、object path、archive read/write 与 digest 校验相关逻辑应归入 application service 或可复用 storage port；本任务优先采用现有 crate 依赖下最小可行边界。
- 不改变 packaged extension artifact 的 API contract、数据库字段或安装语义。
- 不引入旧字段兼容或回退路径。

## Acceptance Criteria

- [ ] `crates/agentdash-api/src/routes/extension_runtime.rs` 不再 import `extension_package_artifacts::read_storage_object`。
- [ ] route-local storage helper 被移动到 application/infrastructure 可复用模块。
- [ ] package upload/install 路径与 runtime webview/archive download 路径通过同一 storage 归属读取 artifact bytes。
- [ ] 相关 API/application tests 或 cargo check 通过。

## Out Of Scope

- 不重做 object storage/cloud storage 适配。
- 不改变 extension artifact 数据模型。
- 不处理 TS Extension Host 权限。
- 不处理 Vite proxy。
