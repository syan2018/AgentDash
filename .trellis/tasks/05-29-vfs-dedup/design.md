# VFS 去重设计

## 目标边界

本任务收敛 VFS application/SPI 层的结构重复，不改变 VFS 地址模型、mount 构建事实源、provider 业务语义或 API 响应形状。

本次涉及：

- `agentdash-spi::platform::mount` 的 provider trait 拆分。
- `agentdash-application::vfs::service` 的 dispatch/helper 化与 typed `MountError` 贯通。
- `agentdash-application::vfs::apply_patch` 的 patch executor 单源化。
- `agentdash-local::ToolExecutor` 迁到同一份 `ApplyPatchTarget` 执行器。
- `workflow::orchestrator` 对 lifecycle output JSON 的严格解析。

## 事实确认

- `VfsService` 当前重复 `resolve_mount -> normalize_mount_relative_path -> registry.get -> ctx -> provider call -> log -> map_err`，`read_text` / `read_binary` / `write_text` / `delete_text` / `rename_text` / `stat` / `list` / `exec` / `search_text_extended` / `grep_text_extended` 都有相同骨架。
- `PROVIDER_INLINE_FS` 在 `service.rs` 内多处用于 overlay 特判；inline provider 本身只负责 DB 文件，session overlay 由 `InlineContentOverlay` 持有。
- `MountProvider` 同时包含 provider metadata、I/O、search、exec、patch、watch、availability。`watch` 没有 provider 消费者；inline overlay 有自己的事件流但不需要挂在 provider SPI 上。
- patch executor 有三份入口：`apply_patch_to_fs`、`apply_patch_to_inline_files`、`apply_entries_to_target`。生产路径只需要 target trait 版本，本机 `ToolExecutor` 是 `apply_patch_to_fs` 的唯一生产调用方。
- `workflow::orchestrator` 当前把 output port 内容 JSON 解析失败降级成 `Value::String`，会吞掉非 text port 的结构错误。

## Provider Trait 拆分

在 `agentdash-spi::platform::mount` 保留 `MountProvider` 作为 trait object 聚合，但将方法分散到三个职责 trait：

```rust
pub trait ProviderDescriptor: Send + Sync {
    fn provider_id(&self) -> &str;
    fn display_name(&self) -> &str { self.provider_id() }
    fn root_ref_hint(&self) -> &str { "" }
    fn supported_capabilities(&self) -> Vec<&str> { vec!["read", "list"] }
    fn is_user_configurable(&self) -> bool { false }
    async fn is_available(&self, _mount: &Mount) -> bool { true }
}

#[async_trait]
pub trait MountIo: ProviderDescriptor {
    async fn read_text(...);
    async fn read_text_range(...);
    async fn read_binary(...);
    async fn write_text(...);
    fn edit_capabilities(...);
    async fn delete_text(...);
    async fn rename_text(...);
    async fn apply_patch(...);
    async fn list(...);
    async fn stat(...);
    async fn exec(...);
}

#[async_trait]
pub trait MountSearch: ProviderDescriptor {
    async fn search_text(...);
    async fn grep_text(...);
    async fn suggest_paths(...);
}

pub trait MountProvider: MountIo + MountSearch {}
impl<T> MountProvider for T where T: MountIo + MountSearch + Send + Sync {}
```

原因：

- 现有 registry 仍可保存 `Arc<dyn MountProvider>`，调用面不需要泛型扩散。
- provider 实现会拆成 `impl ProviderDescriptor` + `impl MountIo` + `impl MountSearch`，职责清楚。
- 默认 `grep_text` / `read_text_range` / `suggest_paths` 逻辑保留在拥有对应职责的 trait 中。
- `watch` / `MountEventReceiver` / `MountEventKind` 从 SPI provider contract 移除。inline overlay 自己的事件类型改为 `InlineOverlayEvent*`，保持 overlay 内部测试与行为，但不再污染 provider SPI。

## VfsService Dispatch Helper

在 `service.rs` 内新增局部 dispatch 支撑类型：

```rust
struct VfsDispatch<'a> {
    mount: &'a Mount,
    path: String,
    provider: Arc<dyn MountProvider>,
    ctx: MountOperationContext,
}
```

核心 helper：

- `resolve_provider_operation(vfs, mount_id, cap, raw_path, allow_empty_dir, identity) -> Result<VfsDispatch, MountError>`
- `dispatch_mount_op(..., op_name, |dispatch| async move { ... })`
- `inline_overlay_read(...)`
- `inline_overlay_write/delete/rename/list/search(...)`

`PROVIDER_INLINE_FS` 只允许出现在一个 helper 中，用于判断当前 mount 是否走 overlay branch。其余 service 方法不再写 provider 字符串分支。

`read_text` / `read_binary` / `write_text` / `delete_text` / `rename_text` / `stat` / `list` / `exec` / `search_text_extended` / `grep_text_extended` 全部改成：

1. 调 dispatch helper。
2. helper 统一 resolve、normalize、provider lookup、ctx、日志。
3. 方法只保留各自业务差异。

## MountError 贯通

`VfsService` 内部 provider-facing 方法改为返回 `MountError`：

- `read_text_range`
- `suggest_paths`
- `read_text`
- `read_binary`
- `write_text`
- `create_text`
- `delete_text`
- `rename_text`
- `stat`
- `apply_patch`
- `apply_multi_mount_patch`
- `list`
- `exec`
- `search_text`
- `search_text_extended`
- `grep_text_extended`
- `grep_inline`

API route、tool facade、materialization、context discovery 等调用边缘按自身错误模型字符串化或映射。这样 `service.rs` 不再以 `map_err(|e| e.to_string())` 抹平 `MountError`，而是让边缘决定显示文本。

## Patch Executor 单源

删除：

- `apply_patch_to_fs`
- `apply_patch_to_inline_files`

保留：

- `parse_patch`
- `apply_patch_to_target`
- `apply_entries_to_target`
- `ApplyPatchTarget`

新增本机 filesystem target：

```rust
pub struct FsPatchTarget {
    mount_root: PathBuf,
}

#[async_trait]
impl ApplyPatchTarget for FsPatchTarget { ... }
```

`agentdash-local::ToolExecutor::apply_patch()` 继续 `spawn_blocking` 包住同步 fs 操作，但调用 `apply_patch_to_target(&FsPatchTarget, patch)`。若 target trait 已是 async，local 可在 blocking 内用当前 runtime handle 或改为 `tokio::fs` 版本；优先实现应用层 `FsPatchTarget` 的同步 fs helper，不引入第二份 patch 语义。

原 `apply_patch_to_fs_*` 单测迁移为 `FsPatchTarget` + `apply_patch_to_target`；`apply_patch_to_inline_files` 测试删除或改成已有 `MemoryTarget` 覆盖。

## Orchestrator JSON 输出

`workflow::orchestrator` 不再把 output port 内容解析失败降级成字符串。

设计：

- 对声明 output port 的内容统一要求是 JSON。
- `serde_json::from_str(&content)` 失败时返回明确错误，包含 port key 与解析错误。
- 如果未来需要 text port，必须由 lifecycle port definition 显式声明 text 类型后单独转换；当前任务不新增兼容降级。

## 风险控制

- 先做 trait 拆分，保持 provider 行为等价。
- 再做 patch 单源，测试覆盖路径逃逸、add/delete/move、capability fallback。
- 再做 `VfsService` dispatch 与 `MountError` 贯通，最后更新调用边缘。
- 每一阶段后优先跑 `cargo check -p agentdash-spi -p agentdash-application`，避免大面积编译错误堆叠。
