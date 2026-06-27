# Design: Canvas Workspace Module 边界预抽取

## Boundary

本子任务只建立纯 crate 边界，不改变运行行为。

新 crate：`agentdash-canvas`

职责：

- Canvas mount id / module id / URI helpers。
- Canvas provider root ref helper。
- Canvas workspace module operation/view/renderer key constants。
- Canvas module ref parsing/building helpers。

不负责：

- Canvas entity / repository。
- Canvas access policy。
- Runtime snapshot resolution。
- RuntimeGateway / VFS read write。
- AgentFrame runtime surface update。
- HTTP route / auth / DTO mapper。

## Proposed API

```rust
pub const CANVAS_MOUNT_ID_PREFIX: &str = "cvs-";
pub const CANVAS_MODULE_ID_PREFIX: &str = "canvas:";
pub const CANVAS_PRESENTATION_SCHEME: &str = "canvas";
pub const CANVAS_PROVIDER_ROOT_SCHEME: &str = "canvas-root";

pub const CANVAS_PREVIEW_VIEW_KEY: &str = "preview";
pub const CANVAS_RENDERER_KIND: &str = "canvas";
pub const CANVAS_BIND_DATA_OPERATION_KEY: &str = "canvas.bind_data";
pub const CANVAS_BIND_DATA_ORIGIN: &str = "host_canvas";

pub fn derive_canvas_mount_id(title: &str) -> String;
pub fn normalize_canvas_mount_id(raw: &str) -> Result<String, CanvasIdentityError>;
pub fn canvas_vfs_mount_id(canvas_mount_id: &str) -> String;
pub fn canvas_module_id(canvas_mount_id: &str) -> String;
pub fn parse_canvas_module_id(module_id: &str) -> Option<&str>;
pub fn canvas_presentation_uri(canvas_mount_id: &str) -> String;
pub fn canvas_vfs_uri(canvas_mount_id: &str, path: &str) -> String;
pub fn canvas_provider_root_ref(canvas_id: impl Display) -> String;
```

`CanvasIdentityError` 使用 crate-local error，不依赖 `agentdash-domain::DomainError`，避免新 crate 反向依赖 domain。application 调用点负责把它映射成 `DomainError` 或 `AgentToolError`。

## Migration Shape

1. 新建 crate manifest 和 `src/lib.rs`。
2. 复制并改造 `agentdash-application/src/canvas/identity.rs` 的纯 helper。
3. 在 `agentdash-application/src/canvas/identity.rs` 保留兼容 re-export/wrapper，减少同一提交的调用点爆炸。
4. 更新 `workspace_module` descriptor 构造逻辑使用新常量。
5. 增加 `agentdash-canvas` 单元测试覆盖：
   - title 派生 mount id。
   - mount id normalize 拒绝空白、缺 `cvs-`、重复前缀、路径分隔符。
   - module id / presentation URI / VFS URI / provider root ref。
6. 运行 workspace check。

## AgentRun Canvas Reference Direction

后续 observation/interaction 状态应使用 AgentRun 到 Canvas 的可见/展示引用作为业务归属。这个子任务先提供稳定 Canvas module ref 和 URI helper，后续子任务再定义具体 `AgentRunCanvasRef` 存储或投影形状。

## Compatibility

项目未上线，不保留旧字段兼容。但本子任务是行为等价迁移，不改变外部 contract。
