# vfs 去重（dispatch helper / MountProvider 拆 trait / patch 收一份 / MountError 贯通）

> parent: `05-29-slop-cleanup-wave2`。盲审病灶 3（D）。类：丙。Wave 3（自包含，可与 W2 并行）。

## Goal

收紧 `application/vfs` 的重复 dispatch、臃肿 trait、三份 patch executor，并让 `MountError` 贯通到边缘再字符串化。

## 现状证据

- `vfs/service.rs` 把 `resolve_mount → normalize_path → if PROVIDER_INLINE_FS {overlay} else {registry.get→ctx→call→log→map_err}` **复制 8~10 遍**（`read_text:188`/`read_binary:221`/`write_text:254`/`delete_text:309`/`rename_text:354`/`stat:431`/`list:696`/`exec:782`/`search_text_extended:836`/`grep_text_extended:898`），inline-fs 魔法字符串分布在每个方法。
- `MountProvider`（`spi/platform/mount.rs`）17 方法，多数默认 `NotSupported`：`watch`/`MountEventReceiver`（无消费者）、`suggest_paths`、`stat`、`read_binary`、`is_available`；并混入 `supported_capabilities`/`display_name`/`root_ref_hint`/`is_user_configurable` 等 UI 配置元数据。
- patch executor 三份（`apply_patch.rs`）：`apply_patch_to_fs:113`（std::fs，仅 `agentdash-local/src/tool_executor.rs:221` 用）、`apply_patch_to_inline_files:199`（**无生产调用方**，死代码）、`apply_entries_to_target:279`（trait 版，service 实际只用它）。
- `MountError`（typed）在 service 几乎每个方法 `.map_err(|e| e.to_string())` 抹平变体；唯 `read_text_range:107` 保留以区分 ENOENT——证明丢失有害。

## Scope

1. 抽单一泛型 `dispatch<F>(mount_id, cap, op, |provider, mount, path| ...)` helper，10 方法复用；inline-fs overlay 下沉为 `InlineFsMountProvider`（或 decorator），service 不再 `== PROVIDER_INLINE_FS` 分支。
2. 拆 `MountProvider` → `MountIo`(read/write/list/delete/rename) + 可选 `MountSearch` + `ProviderDescriptor`(UI 元数据)；删 `watch` 至有消费者。
3. 删死的 `apply_patch_to_inline_files`；`agentdash-local` 实现 `ApplyPatchTarget` over std::fs 走 `apply_entries_to_target`，再删 `apply_patch_to_fs`。
4. `VfsService` 统一返回 `MountError`（或薄 app-error 包装），只在 tool/HTTP 边缘字符串化。
5. `materialization`/`orchestrator.rs:255` 的 `serde_json::from_str(..).unwrap_or_else(|_| Value::String(..))` 静默吞 JSON 解析失败 → 仅声明 text port 保留原串，否则报错。

## Acceptance Criteria（硬指标 + 验收命令）

- [ ] `rg "PROVIDER_INLINE_FS" crates/agentdash-application/src/vfs/service.rs | wc -l` ≤ **1**（inline-fs 分支下沉，10 方法走单一 dispatch helper）
- [ ] `MountProvider` 拆为 `MountIo`/`MountSearch`/`ProviderDescriptor` 三 trait（`rg "trait MountIo|trait MountSearch|trait ProviderDescriptor"` 三命中）；`rg "fn watch|MountEventReceiver" crates/agentdash-spi crates/agentdash-application/src/vfs` = **0**
- [ ] `rg "fn apply_patch_to_inline_files|fn apply_patch_to_fs" crates` = **0**（仅 `apply_entries_to_target` 留存）
- [ ] `rg "map_err\(\|e\| e.to_string\(\)\)" crates/agentdash-application/src/vfs/service.rs | wc -l` ≤ **1**（`MountError` 贯通，仅边缘字符串化）
- [ ] `orchestrator.rs` 不再 `unwrap_or_else(|_| Value::String` 静默吞 JSON（grep = 0 或仅 text port 保留并注释）
- [ ] `cargo check --workspace` exit 0 + vfs 相关测试通过
