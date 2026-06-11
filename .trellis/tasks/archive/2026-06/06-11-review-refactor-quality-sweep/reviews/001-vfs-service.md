# REVIEW-001: vfs-service

## 范围

- `crates/agentdash-application/src/vfs/service.rs`
- `crates/agentdash-application/src/vfs/tools/**`
- 相关 API surface resolver/helper 边界

## 实现级可修复问题

### VFS-IMPL-001: `create_text` 吞掉非 NotFound 错误

- 证据：`crates/agentdash-application/src/vfs/service.rs:363` `create_text` 将 `read_text` 的任意 `Err(_)` 都当成“不存在或不可读”，随后继续 `write_text`。
- 影响：`NotSupported`、backend 离线、provider 内部错误、权限类错误会被 create 流程吞掉一轮，错误语义不稳定，也可能触发非预期写入尝试。
- 建议：只把 `MountError::NotFound` 视为可创建，其余错误直接返回；必要时补 provider mock 测试覆盖 `NotSupported/Unavailable`。

### VFS-IMPL-002: search/grep 路径未传递 identity

- 证据：`crates/agentdash-application/src/vfs/service.rs:952`、`1018` 的 `search_text_extended` / `grep_text_extended` 调 `resolve_provider_dispatch(..., None)`；`crates/agentdash-application/src/vfs/tools/fs/grep.rs:45` 的 `FsGrepTool::new` 接收 `_identity` 但直接丢弃。
- 影响：read/list/write 路径已传递 identity，但 search/grep 路径不传，provider 若依赖 `MountOperationContext.identity` 做访问控制或租户过滤，会出现同一 mount 不同操作权限不一致。
- 建议：`TextSearchParams` 增加 `identity` 或 service 方法显式参数；`FsGrepTool` 存储并传入 identity，`grep_inline` 也不要用 `MountOperationContext::default()`。

### VFS-IMPL-003: patch 路径拆 mount 前缀逻辑重复

- 证据：`crates/agentdash-application/src/vfs/service.rs:1247` 与 `crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs:215` 分别实现 `split_mount_prefix` 和 `mutation_key_parts`。
- 影响：锁粒度和实际执行路径可能随未来改动漂移，导致 mutation queue 锁住的 key 与真正修改的文件不一致。
- 建议：抽出共享 `PatchPathTarget { mount_id, relative_path }` helper，锁 key 与执行分组共用同一解析结果。

### VFS-IMPL-004: runtime file metadata key 裸字符串重复

- 证据：`crates/agentdash-application/src/vfs/service.rs:1288`、`crates/agentdash-application/src/vfs/tools/fs/read.rs:360`、`crates/agentdash-api/src/routes/vfs_surfaces/helpers.rs:68` 重复解析 `RuntimeFileEntry.attributes["content_kind"] / ["mime_type"]`。
- 影响：metadata 语义是跨 provider/tool/API 的契约，重复字符串会让新增 content kind 或 key 调整时容易漏改。
- 建议：在 application VFS 暴露统一 accessor，例如 `runtime_entry_content_kind` / `runtime_entry_mime_type` 或 typed view，API 与 tools 复用。

### VFS-IMPL-005: tool 层默认 mount 路径未统一 normalize

- 证据：`crates/agentdash-application/src/vfs/tools/common.rs:15` 的 `resolve_uri_path` 对无 `://` 的默认 mount 路径直接返回 `trimmed.to_string()`。
- 影响：tool 层 dedup key、错误提示、后续 mutation key 可能拿到未 normalize 的路径；normalize 被推迟到 service/provider，多处重复兜底。
- 建议：无前缀路径也通过 `MountRelativePath::parse(..., true)` 或统一 `VfsUri::parse`，让 tool 边界输出已规范化 `ResourceRef`。

### VFS-IMPL-006: `VfsService` 职责过宽

- 证据：`crates/agentdash-application/src/vfs/service.rs:59` 的 `VfsService` 同时承载 dispatch、overlay 读写、stat fallback、apply_patch、multi-mount patch、list overlay merge、exec、search/grep 格式化，文件长约 1491 行。
- 影响：局部修复经常触碰 unrelated search/patch/list 行为；inline overlay 分支散落在多个方法中。
- 建议：按职责拆出 `MountDispatcher`、`InlineOverlayView`、`VfsPatchService`、`VfsSearchService`，`VfsService` 保留门面或 use case 编排。

## 架构 backlog 候选

### VFS-ARCH-001: runtime tool composition root 落在 VFS 模块

- 证据：`crates/agentdash-application/src/vfs/tools/provider.rs:57` 名为 `RelayRuntimeToolProvider` 且位于 `vfs/tools`，但实际装配 VFS、shell、workflow、companion、canvas、workspace module 等工具，核心逻辑从 `188` 到 `471`。
- 影响：VFS 模块成为 runtime tool composition root，跨 canvas/workflow/companion/workspace module 的依赖都汇入 VFS，模块边界被稀释。
- 建议：建立 session/runtime 级 `RuntimeToolProviderComposer`；VFS 只提供 VFS tool factory，其他领域在各自模块注册工具集合。

### VFS-ARCH-002: inline mutation 存在 API 与 Agent runtime 两套语义

- 证据：`crates/agentdash-application/src/vfs/mutation_dispatcher.rs:97` inline 写入直接走 repo/storage key；`crates/agentdash-application/src/vfs/service.rs:310` agent tool overlay 写入走 `InlineContentOverlay`。
- 影响：API surface mutation 和 Agent runtime mutation 对 inline_fs 的事实源、冲突语义、persisted 语义不完全同源。
- 建议：收敛为一个 inline mutation port/use case，overlay 只表达 session 暂存层，持久化写入统一经过 dispatcher 或更底层 inline storage writer。

### VFS-ARCH-003: API resolver 承担 VFS surface resolution 编排

- 证据：`crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:33` 直接组合 project/story/task/workspace/project_vfs_mount repos，并调用 application VFS builder；surface summary 又在同文件 `226` 通过 API adapter 回调 application。
- 影响：API 层仍承担大量 surface resolution 编排，application 层的 VFS surface use case 边界不够完整。
- 建议：把 `resolve_surface_bundle` 收敛进 application service，API 只传入鉴权后的 source/permission 与 runtime projection adapter。
