# Research: VFS service executable plan

- Query: 基于 `.trellis/tasks/06-11-review-refactor-quality-sweep/reviews/001-vfs-service.md`，把 VFS 剩余模块级问题整理为可执行、可并行的修复批次，并按“超过 10 个文件或跨事实源/公共 contract/数据库/跨层协议”门槛区分架构项。
- Scope: internal
- Date: 2026-06-11

## Findings

### Source Context

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本研究使用用户显式给出的任务目录 `.trellis/tasks/06-11-review-refactor-quality-sweep`，只写入该目录的 `research/`。
- 根目录 `reviews/001-vfs-service.md` 不存在；实际 review 位于 `.trellis/tasks/06-11-review-refactor-quality-sweep/reviews/001-vfs-service.md`。
- `.trellis/tasks/06-11-review-refactor-quality-sweep/fixes/002-vfs-create-text-error-semantics.md` 显示 VFS-IMPL-001 已提交修复，本研究只规划 VFS-IMPL-002 到 VFS-IMPL-006 与 VFS-ARCH-001。

### Related Specs

- `.trellis/workflow.md`：research 必须持久化到任务 `research/`；Phase 2 由 implement/check sub-agent 执行。
- `.trellis/spec/backend/vfs/architecture.md`：VFS tool module baseline 要求共享 runtime VFS handle 和 URI resolution 在 `vfs/tools/common.rs`，file/search/patch/shell handler 在 `vfs/tools/fs/`。
- `.trellis/spec/backend/vfs/vfs-access.md`：外部地址统一为 `surface_ref + mount_id + mount_relative_path`；`mount_relative_path` 进入 application 前必须 normalize；runtime tools 使用 `{ "mount": "main", "path": "relative/path" }`。
- `.trellis/spec/backend/vfs/vfs-materialization.md`：shell/VFS URI 物化遵守稳定 key 和 mount-relative 路径规则。
- `.trellis/spec/backend/error-handling.md`：层边界保留结构化错误语义，不应通过字符串猜测。
- `.trellis/spec/backend/domain-payload-typing.md`：高频业务路径应逐步类型化，避免重复裸 `serde_json::Value` key。
- `.trellis/spec/backend/session/session-startup-pipeline.md`：runtime tool provider 是 AppState ready gate 的主链路依赖，迁移 composer 会影响 session 启动链路。
- `.trellis/spec/backend/runtime-gateway.md`：runtime action/tool adapter 不能自行做 capability 裁决，工具注入策略仍要由 session/runtime capability surface 驱动。
- `.trellis/spec/cross-layer/desktop-local-runtime.md`：relay/local runtime 的 session workspace/VFS facts 由 launch projection 传递，不应在 runtime tool 层重新猜测。
- `.trellis/spec/guides/code-reuse-thinking-guide.md`：重复 helper / contract key 应先搜索，再收敛到 owning module。

### Code Patterns

- `VfsService::resolve_provider_dispatch` 已支持 `identity` 并写入 `MountOperationContext { identity: identity.cloned() }`：`crates/agentdash-application/src/vfs/service.rs:107`, `crates/agentdash-application/src/vfs/service.rs:125`。
- read/list/write/stat/patch 多数路径已传 `identity` 到 dispatch 或 provider context：`crates/agentdash-application/src/vfs/service.rs:151`, `crates/agentdash-application/src/vfs/service.rs:232`, `crates/agentdash-application/src/vfs/service.rs:796`, `crates/agentdash-application/src/vfs/service.rs:641`。
- `TextSearchParams` 当前没有 identity 字段：`crates/agentdash-application/src/vfs/service.rs:35`。
- `search_text_extended` 和 `grep_text_extended` 对 search dispatch 传 `None`：`crates/agentdash-application/src/vfs/service.rs:950`, `crates/agentdash-application/src/vfs/service.rs:955`, `crates/agentdash-application/src/vfs/service.rs:1016`, `crates/agentdash-application/src/vfs/service.rs:1021`。
- inline grep/list/read 使用 `MountOperationContext::default()`，导致 inline 分支同样丢 identity：`crates/agentdash-application/src/vfs/service.rs:1085`, `crates/agentdash-application/src/vfs/service.rs:1096`, `crates/agentdash-application/src/vfs/service.rs:1102`, `crates/agentdash-application/src/vfs/service.rs:1109`。
- `FsGrepTool::new` 接收 `_identity` 后丢弃：`crates/agentdash-application/src/vfs/tools/fs/grep.rs:39`, `crates/agentdash-application/src/vfs/tools/fs/grep.rs:45`, `crates/agentdash-application/src/vfs/tools/fs/grep.rs:49`；provider composer 已把 `identity.clone()` 传给 grep tool：`crates/agentdash-application/src/vfs/tools/provider.rs:216`。
- patch 执行分组使用 `normalize_patch_entry_paths` / `split_mount_prefix`：`crates/agentdash-application/src/vfs/service.rs:677`, `crates/agentdash-application/src/vfs/service.rs:701`, `crates/agentdash-application/src/vfs/service.rs:1245`, `crates/agentdash-application/src/vfs/service.rs:1265`。
- patch tool 锁 key 使用另一套 `mutation_key_parts`：`crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs:175`, `crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs:193`, `crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs:215`。
- runtime file metadata key 重复解析：`crates/agentdash-application/src/vfs/service.rs:547`, `crates/agentdash-application/src/vfs/service.rs:1106`, `crates/agentdash-application/src/vfs/service.rs:1286`, `crates/agentdash-application/src/vfs/tools/fs/read.rs:145`, `crates/agentdash-application/src/vfs/tools/fs/read.rs:360`, `crates/agentdash-application/src/vfs/tools/fs/read.rs:369`, `crates/agentdash-api/src/routes/vfs_surfaces/helpers.rs:54`, `crates/agentdash-api/src/routes/vfs_surfaces/helpers.rs:68`, `crates/agentdash-api/src/routes/vfs_surfaces/helpers.rs:77`。
- SPI 内已有仅 crate 内可见的 `entry_is_binary`，说明 metadata 判断已有 owning-type 候选但未暴露给 application/API：`crates/agentdash-spi/src/platform/mount.rs:165`, `crates/agentdash-spi/src/platform/mount.rs:217`。
- `resolve_uri_path` 对默认 mount / 单 mount 的无前缀路径直接返回 `trimmed.to_string()`：`crates/agentdash-application/src/vfs/tools/common.rs:15`, `crates/agentdash-application/src/vfs/tools/common.rs:25`, `crates/agentdash-application/src/vfs/tools/common.rs:34`。
- typed URI 解析已经能 normalize 无前缀路径并 resolve links：`crates/agentdash-application/src/vfs/path.rs:87`, `crates/agentdash-application/src/vfs/path.rs:102`, `crates/agentdash-application/src/vfs/path.rs:193`。
- `VfsService` 当前 1489 行，集中 dispatch、overlay、stat fallback、patch、list、exec、search/grep 与 helper：`crates/agentdash-application/src/vfs/service.rs:59`, `crates/agentdash-application/src/vfs/service.rs:518`, `crates/agentdash-application/src/vfs/service.rs:677`, `crates/agentdash-application/src/vfs/service.rs:790`, `crates/agentdash-application/src/vfs/service.rs:888`, `crates/agentdash-application/src/vfs/service.rs:950`, `crates/agentdash-application/src/vfs/service.rs:1016`。
- `RelayRuntimeToolProvider` 位于 VFS 模块但 import 并装配 VFS、shell、workflow、companion、canvas、workspace module：`crates/agentdash-application/src/vfs/tools/provider.rs:18`, `crates/agentdash-application/src/vfs/tools/provider.rs:29`, `crates/agentdash-application/src/vfs/tools/provider.rs:58`, `crates/agentdash-application/src/vfs/tools/provider.rs:160`, `crates/agentdash-application/src/vfs/tools/provider.rs:188`, `crates/agentdash-application/src/vfs/tools/provider.rs:264`, `crates/agentdash-application/src/vfs/tools/provider.rs:280`, `crates/agentdash-application/src/vfs/tools/provider.rs:309`, `crates/agentdash-application/src/vfs/tools/provider.rs:376`。
- runtime tool provider 装配进入 API bootstrap 和 session runtime ready gate：`crates/agentdash-api/src/bootstrap/vfs.rs:86`, `crates/agentdash-api/src/bootstrap/session.rs:127`, `crates/agentdash-application/src/session/hub/factory.rs:263`, `crates/agentdash-application/src/session/hub/tool_builder.rs:274`, `crates/agentdash-application/src/session/launch/deps.rs:181`。

### Files Found

| Batch | Files | Description |
| --- | --- | --- |
| A: search/grep identity | `crates/agentdash-application/src/vfs/service.rs` | Add identity to `TextSearchParams`, pass it through `search_text_extended` / `grep_text_extended` / `grep_inline`. |
| A: search/grep identity | `crates/agentdash-application/src/vfs/tools/fs/grep.rs` | Store identity in `FsGrepTool` and pass it into `TextSearchParams`. |
| A: search/grep identity | `crates/agentdash-api/src/vfs_access/mod.rs` | Update direct `VfsService::search_text` test calls if the wrapper signature gains identity. |
| B: patch path target helper | `crates/agentdash-application/src/vfs/apply_patch.rs` | Own shared patch path parsing/normalization helper next to `PatchEntry` parsing. |
| B: patch path target helper | `crates/agentdash-application/src/vfs/service.rs` | Replace local `split_mount_prefix` / `normalize_patch_entry_paths` with shared helper. |
| B: patch path target helper | `crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs` | Replace local `mutation_key_parts` with the same helper used by service execution. |
| B: patch path target helper | `crates/agentdash-application/src/vfs/mod.rs` | Re-export helper only at crate/application VFS boundary if needed by `tools/fs/apply_patch.rs`. |
| C: runtime metadata accessors | `crates/agentdash-application/src/vfs/types.rs` | Add VFS-owned constants/accessors for `RuntimeFileEntry.attributes`. |
| C: runtime metadata accessors | `crates/agentdash-application/src/vfs/service.rs` | Use accessor for binary skip and text overlay metadata creation. |
| C: runtime metadata accessors | `crates/agentdash-application/src/vfs/tools/fs/read.rs` | Use accessor for binary/image routing and MIME lookup. |
| C: runtime metadata accessors | `crates/agentdash-api/src/routes/vfs_surfaces/helpers.rs` | Remove local metadata key parsing; map application accessor result into DTO fields. |
| D: tool path normalize | `crates/agentdash-application/src/vfs/tools/common.rs` | Resolve all tool paths through typed mount URI parsing, including default mount paths. |
| D: tool path normalize | `crates/agentdash-application/src/vfs/path.rs` | No core change expected; existing tests document typed normalization. Add tests only if behavior gap is found. |
| E: VFS tool factory phase | `crates/agentdash-application/src/vfs/tools/provider.rs` | Keep composer in place but delegate VFS read/write/execute tool assembly to a VFS-specific factory. |
| E: VFS tool factory phase | `crates/agentdash-application/src/vfs/tools/factory.rs` | New small factory for `mounts_list`, `fs_read`, `fs_glob`, `fs_grep`, `fs_apply_patch`, `shell_exec`. |
| E: VFS tool factory phase | `crates/agentdash-application/src/vfs/tools/mod.rs` | Export the new VFS factory if needed inside application. |
| F: narrow VfsService split | `crates/agentdash-application/src/vfs/search.rs` | Optional post-cleanup extraction for search params/formatting/inline grep only. |
| F: narrow VfsService split | `crates/agentdash-application/src/vfs/service.rs` | Delegate search/grep helpers; do not move dispatcher, overlay, list, exec or patch broadly. |
| F: narrow VfsService split | `crates/agentdash-application/src/vfs/mod.rs` | Export `TextSearchParams` from its new owner if moved. |

## Immediate Implementation Batches

### Batch A: VFS-IMPL-002 search/grep identity propagation

- Parallelism: sequential before Batch F; can run independently from Batch D and Batch E. It conflicts with Batch B/C/F on `service.rs`.
- Write scope:
  - `crates/agentdash-application/src/vfs/service.rs`
  - `crates/agentdash-application/src/vfs/tools/fs/grep.rs`
  - `crates/agentdash-api/src/vfs_access/mod.rs` only if `VfsService::search_text` gains an identity argument.
- Core changes:
  - Add `identity: Option<&'a agentdash_spi::platform::auth::AuthIdentity>` to `TextSearchParams<'a>`.
  - Update `VfsService::search_text` to accept `identity` and pass it into `TextSearchParams`; update the two API test calls currently at `crates/agentdash-api/src/vfs_access/mod.rs:454` and `crates/agentdash-api/src/vfs_access/mod.rs:565` with `None`.
  - Pass `params.identity` into `resolve_provider_dispatch` in `search_text_extended` and `grep_text_extended` instead of `None`.
  - Replace `MountOperationContext::default()` in `grep_inline` with a context cloned from `params.identity`.
  - Add `identity` field to `FsGrepTool`, store the constructor argument currently named `_identity`, and set it in the `TextSearchParams` used at `crates/agentdash-application/src/vfs/tools/fs/grep.rs:182`.
  - Add targeted tests with an identity-capturing `MountProvider`: one for `search_text_extended`, one for `grep_text_extended`, and one inline-provider shaped test proving `grep_inline` passes identity through `list` / `read_text`.
- Risk:
  - `TextSearchParams` is public from `agentdash-application::vfs`; update all in-repo constructors. Current search shows `FsGrepTool` and `VfsService::search_text` are the practical call sites.
  - Inline branch tests need a custom provider with provider id `inline_fs`; otherwise the built-in inline provider ignores context and will not detect regressions.
- Validation commands:
  - `cargo test -p agentdash-application search_identity`
  - `cargo test -p agentdash-application fs_grep`
  - `cargo test -p agentdash-api vfs_access`

### Batch B: VFS-IMPL-003 shared patch path target parsing

- Parallelism: after or serialized with Batch A/C because all touch `service.rs`; independent from Batch D/E.
- Write scope:
  - `crates/agentdash-application/src/vfs/apply_patch.rs`
  - `crates/agentdash-application/src/vfs/service.rs`
  - `crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs`
  - `crates/agentdash-application/src/vfs/mod.rs`
- Core changes:
  - Introduce a shared helper close to patch grammar, for example:
    - `PatchPathTarget { mount_id: String, relative_path: String }`
    - `parse_patch_path_target(raw, fallback_mount_id) -> Result<PatchPathTarget, String>`
    - `normalize_patch_entry_targets(entry, fallback_mount_id) -> Result<PatchPathTarget, String>`
  - Move the existing `split_mount_prefix` semantics from service into that helper and make tool lock-key collection call the same helper.
  - Ensure service grouping and lock-key generation both normalize `main://src//a.rs`, bare `src/a.rs`, and move targets the same way.
  - Keep cross-mount move rejection in the shared helper when mutating entries for execution. If lock key collection still wants to inspect cross-mount move patches before execution, return the same error instead of silently diverging.
  - Consider changing `FsApplyPatchTool::execute` so lock-key parse errors return `AgentToolError::ExecutionFailed` rather than `unwrap_or_default()` at `crates/agentdash-application/src/vfs/tools/fs/apply_patch.rs:126`; this keeps invalid patch paths from running unlocked.
- Risk:
  - The existing tool lock key tests accept explicit cross-mount target keys; service execution rejects cross-mount move. The helper must distinguish “multi-mount patch with independent entries” from “single move crossing mounts”.
  - `PatchEntry` paths are `PathBuf`; always convert through `to_string_lossy()` before parsing to match existing behavior.
- Validation commands:
  - `cargo test -p agentdash-application apply_patch_mutation_keys`
  - `cargo test -p agentdash-application patch_entry`
  - `cargo test -p agentdash-application fs_apply_patch`

### Batch C: VFS-IMPL-004 runtime file metadata accessors

- Parallelism: serialize with Batch A/B/F if service edits overlap; otherwise small enough for a separate implement pass.
- Write scope:
  - `crates/agentdash-application/src/vfs/types.rs`
  - `crates/agentdash-application/src/vfs/service.rs`
  - `crates/agentdash-application/src/vfs/tools/fs/read.rs`
  - `crates/agentdash-api/src/routes/vfs_surfaces/helpers.rs`
- Core changes:
  - Add application VFS-owned metadata contract helpers in `types.rs`, for example:
    - `RUNTIME_FILE_CONTENT_KIND_ATTR`
    - `RUNTIME_FILE_MIME_TYPE_ATTR`
    - `RUNTIME_FILE_CONTENT_KIND_TEXT`
    - `RUNTIME_FILE_CONTENT_KIND_BINARY`
    - `runtime_entry_content_kind(&RuntimeFileEntry) -> Option<&str>`
    - `runtime_entry_mime_type(&RuntimeFileEntry) -> Option<&str>`
    - `runtime_entry_is_binary(&RuntimeFileEntry) -> bool`
    - optional `runtime_text_file_attributes()`.
  - Replace local `entry_content_kind` in service and `entry_content_kind` / `entry_mime_type` in fs_read.
  - Replace API helper parsing in `surface_stat_response`; keep DTO output as `Option<String>` by mapping borrowed `&str` to owned string at the edge.
  - Do not change the wire shape of `RuntimeFileEntry.attributes` in this batch. This is a key/accessor consolidation, not a public contract migration.
- Risk:
  - Provider attribute construction also repeats these strings in `provider_inline.rs`, `provider_skill_asset.rs`, `provider_routine.rs`, and `mutation_dispatcher.rs`. If implementation expands to those producers, the batch remains module-level but touches more files. Keep it under 10 files and avoid SPI/public DTO changes.
  - `agentdash-spi::platform::mount::entry_is_binary` is currently `pub(crate)`; do not depend on it from application unless intentionally moving the owning accessor to SPI.
- Validation commands:
  - `cargo test -p agentdash-application fs_read`
  - `cargo test -p agentdash-application vfs::`
  - `cargo test -p agentdash-api vfs_access`

### Batch D: VFS-IMPL-005 normalize default mount paths at tool boundary

- Parallelism: can run in parallel with Batch B/C/E because it only writes `vfs/tools/common.rs` in the expected path. It may change `fs_read` / `fs_glob` / `fs_grep` / `shell_exec` behavior, so run focused tool tests after merge.
- Write scope:
  - `crates/agentdash-application/src/vfs/tools/common.rs`
  - `crates/agentdash-application/src/vfs/path.rs` only if existing parser behavior needs an extra regression test.
- Core changes:
  - Replace default mount and single mount branches that return `trimmed.to_string()` with typed parsing through `parse_mount_uri(trimmed, vfs)` or `VfsUri::parse(..., allow_empty_path = true).into_resource_ref()` plus link resolution.
  - Update `unqualified_path_uses_default_mount_when_available`: `.` should normalize to empty mount root (`""`), not `"."`.
  - Add regression tests:
    - `resolve_uri_path(&vfs, "./src//lib.rs")` returns `path == "src/lib.rs"`.
    - `resolve_uri_path(&vfs, "../secret")` rejects.
    - `resolve_uri_path(&vfs, "C:/repo/file.rs")` rejects on Windows-like absolute path.
  - Preserve the “ambiguous no default mount” error semantics if possible; if using `parse_mount_uri` changes the message, update the test to assert the stable meaning rather than the old English phrase.
- Risk:
  - Existing tools may have tests that assert `"."` as path text; service/provider paths already normalize root with `allow_empty = true`, so root-as-empty is the correct VFS boundary shape.
  - `resolve_uri_path` currently imports `parse_mount_uri`; no new dependency is needed.
- Validation commands:
  - `cargo test -p agentdash-application resolve_uri_path`
  - `cargo test -p agentdash-application fs_glob`
  - `cargo test -p agentdash-application fs_grep`
  - `cargo test -p agentdash-application fs_read`

### Batch E: VFS-ARCH-001 phase 0, VFS tool factory extraction only

- Parallelism: can run in parallel with Batch A/B/C/D if it does not rename `RelayRuntimeToolProvider` or move `SessionToolServices`. It writes `vfs/tools/provider.rs` and new `vfs/tools/factory.rs`.
- Write scope:
  - `crates/agentdash-application/src/vfs/tools/provider.rs`
  - `crates/agentdash-application/src/vfs/tools/factory.rs`
  - `crates/agentdash-application/src/vfs/tools/mod.rs`
- Core changes:
  - Add a VFS-owned factory that builds only:
    - `mounts_list`
    - `fs_read`
    - `fs_glob`
    - `fs_grep`
    - `fs_apply_patch`
    - `shell_exec`
  - Keep capability checks identical: use `flow.is_capability_tool_enabled(...)` with the same `CAP_FILE_READ`, `CAP_FILE_WRITE`, and `CAP_SHELL_EXECUTE` keys currently used at `crates/agentdash-application/src/vfs/tools/provider.rs:188` to `crates/agentdash-application/src/vfs/tools/provider.rs:261`.
  - Pass through existing shared inputs: `Arc<VfsService>`, `SharedRuntimeVfs`, overlay, identity, materialization service, session id/turn id, and shell output registry.
  - Leave workflow, companion, canvas, workspace module assembly in `RelayRuntimeToolProvider` for now. This makes the VFS tool factory available without pretending the composition root migration is complete.
- Risk:
  - Avoid moving `SessionToolServices` in this batch. Canvas/companion/workflow currently import it from `crate::vfs::tools`, and moving it triggers cross-module churn.
  - Keep constructor signatures stable so `agentdash-api/src/bootstrap/vfs.rs` does not change.
- Validation commands:
  - `cargo check -p agentdash-application`
  - `cargo test -p agentdash-application fs_grep`
  - `cargo test -p agentdash-application fs_read`

### Batch F: VFS-IMPL-006 narrow `VfsService` split, search only

- Parallelism: run after Batch A and Batch C. Do not run in parallel with Batch B if both edit `service.rs` / `mod.rs`.
- Write scope:
  - `crates/agentdash-application/src/vfs/search.rs`
  - `crates/agentdash-application/src/vfs/service.rs`
  - `crates/agentdash-application/src/vfs/mod.rs`
  - `crates/agentdash-application/src/vfs/tools/fs/grep.rs` only if `TextSearchParams` import path changes.
- Core changes:
  - Extract only search/grep-specific code:
    - `TextSearchParams`
    - long-line trim helper
    - search/grep match formatting
    - inline grep/search helper that needs mount provider registry and `InlineContentOverlay`
  - Keep `VfsService::search_text`, `search_text_extended`, and `grep_text_extended` public as facade methods to avoid touching API/tool call sites beyond imports.
  - Do not attempt the full review suggestion of `MountDispatcher`, `InlineOverlayView`, `VfsPatchService`, `VfsSearchService` in one pass. Those names describe a future direction, but the quick convergent split is search-only because Batch A already establishes the final identity shape.
- Risk:
  - Search code depends on `MountProviderRegistry`, inline provider detection, `ListOptions`, `SearchQuery`, `GrepQuery`, `SearchOutputMode`, and metadata binary checks. Extract after Batch C so the binary check uses the new accessor.
  - If the extraction grows past 4 files or changes service method signatures, stop and leave it for a separate design pass.
- Validation commands:
  - `cargo test -p agentdash-application search_text_extended`
  - `cargo test -p agentdash-application fs_grep`
  - `cargo test -p agentdash-application vfs::`

## Architecture Backlog

### ARCH: full runtime tool composer migration out of VFS

- Status: backlog; Batch E is only the VFS factory pre-step.
- Evidence:
  - `RelayRuntimeToolProvider` lives in `crates/agentdash-application/src/vfs/tools/provider.rs:58`.
  - The same `build_tools` assembles VFS, shell, workflow, companion, canvas, and workspace module tools: `crates/agentdash-application/src/vfs/tools/provider.rs:188`, `crates/agentdash-application/src/vfs/tools/provider.rs:264`, `crates/agentdash-application/src/vfs/tools/provider.rs:280`, `crates/agentdash-application/src/vfs/tools/provider.rs:309`, `crates/agentdash-application/src/vfs/tools/provider.rs:376`.
  - Runtime provider wiring participates in API bootstrap and session runtime ready gate: `crates/agentdash-api/src/bootstrap/vfs.rs:86`, `crates/agentdash-api/src/bootstrap/session.rs:127`, `crates/agentdash-application/src/session/hub/factory.rs:263`, `crates/agentdash-application/src/session/hub/tool_builder.rs:274`, `crates/agentdash-application/src/session/launch/deps.rs:181`.
- Why it meets the architecture threshold:
  - Full migration likely touches more than 10 files: `vfs/tools/provider.rs`, `vfs/tools/mod.rs`, `api/bootstrap/vfs.rs`, `api/bootstrap/session.rs`, `api/app_state.rs`, `session/runtime_builder.rs`, `session/hub/factory.rs`, `session/hub/tool_builder.rs`, `session/launch/deps.rs`, `canvas/tools.rs`, `companion/tools.rs`, `workspace_module/tools.rs`, `workflow/tools/advance_node.rs`, and related tests.
  - It crosses module ownership: VFS, session launch, API bootstrap, workflow, companion, canvas, workspace module, runtime gateway handles.
  - It changes the composition root for runtime tool capability surface, which is part of session startup and AppState readiness.
- Phased direction:
  - Phase 0 now: implement Batch E so VFS has a real VFS tool factory while the current composer still calls it.
  - Phase 1 design: introduce `RuntimeToolProviderComposer` under session/runtime ownership. It consumes small domain factories rather than importing every tool directly from VFS.
  - Phase 2 design: move `SessionToolServices` / `SharedSessionToolServicesHandle` out of `vfs::tools` to a session-owned module so canvas/companion/workflow stop depending on VFS for session service handles.
  - Phase 3 migration: update API bootstrap so `build_vfs_kernel` returns VFS kernel pieces only, and session/bootstrap builds the runtime tool composer.
- Not included in immediate implementation because the full migration exceeds file-count and cross-module boundaries. The VFS factory pre-step is immediate because it stays within `crates/agentdash-application/src/vfs/tools/` and does not alter runtime tool provider ownership.

## Non-Deferred Review Items

- VFS-IMPL-002 is not architecture. The SPI already has `MountOperationContext.identity`, and `VfsService::resolve_provider_dispatch` already accepts identity. The missing work is plumbing through `TextSearchParams`, `FsGrepTool`, and inline grep.
- VFS-IMPL-003 is not architecture. Both duplicated patch path functions are inside application VFS and can be replaced by a shared helper close to `PatchEntry` parsing.
- VFS-IMPL-004 is not architecture if limited to constants/accessors. Do not turn it into a public `RuntimeFileEntry` DTO migration in this pass.
- VFS-IMPL-005 is not architecture. `parse_mount_uri` / `VfsUri::parse` already provide the correct typed normalization behavior; tool common just bypasses it for default mount paths.
- VFS-IMPL-006 should not be deferred wholesale. The broad service split proposal is too large, but a search-only extraction after identity/metadata cleanup is a bounded module-level refactor. Patch path and metadata batches also reduce `service.rs` responsibility without a large rewrite.
- VFS-ARCH-001 phase 0 is not architecture. Extracting a VFS tool factory inside `vfs/tools/` is a small module cleanup. Moving the actual composer and session service handles out of VFS is the architecture backlog item.

## External References

- No web references used. This research is based on local code, local Trellis specs, and current crate structure.
- Local Rust validation targets found: `agentdash-application` and `agentdash-api` from `Cargo.toml` / crate manifests.

## Caveats / Not Found

- Tests were not run; this is research-only. Validation commands above are proposed for implement/check agents.
- Line numbers reflect the working tree during this research pass.
- Full `VfsService` decomposition into `MountDispatcher`, `InlineOverlayView`, `VfsPatchService`, and `VfsSearchService` was intentionally not recommended as one immediate batch because it would likely expand beyond quick module-level convergence.
- Existing architecture backlog already contains inline mutation as a true architecture item. This research did not re-plan that item because the user focus was VFS-IMPL-002 through VFS-IMPL-006 and VFS-ARCH-001.
