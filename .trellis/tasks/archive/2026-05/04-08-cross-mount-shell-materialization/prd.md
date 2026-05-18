# VFS 资源本地物化与 URL 重定向

> 状态：planning  
> 优先级：P1  
> 任务重写日期：2026-05-13  
> 前置依赖：`05-12-skill-assets-category` 已完成并归档。

## 背景

Agent 现在可以用 VFS URI 引用 session 可见资源，例如：

```text
skill-assets://skills/reviewer/scripts/check.sh
lifecycle://skills/reviewer/references/rules.md
```

这些 URI 对 Agent 和 VFS provider 是稳定标识，但本机 shell、本机 MCP server、浏览器预览和部分 URL-only 消费者只能处理本机 path 或 HTTP URL。

当前代码里：

- `crates/agentdash-application/src/vfs/tools/fs.rs` 的 `fs_read` / `fs_glob` / `fs_grep` / `shell_exec.cwd` 会把完整 path 参数解析成 `ResourceRef`。
- `ShellExecTool` 只把 `params.command` 原样放进 `ExecRequest.command`。
- `crates/agentdash-api/src/mount_providers/relay_fs.rs` 把 exec 转成 `CommandToolShellExec`，只传 `command`、`mount_root_ref`、`cwd`、`timeout_ms`。
- `crates/agentdash-local/src/tool_executor.rs` 只在本机文件系统里执行命令，不认识 `skill-assets://` 或 `lifecycle://`。

因此，云端 VFS 能读到资源，不代表本机 shell 能打开这些 URI。物化必须把资源内容通过 relay 明确落到执行机器上的 session cache，再把命令、MCP 参数或 URL 改写成这个本机可访问位置。

## Goal

1. 为 session VFS 中的只读资源提供本机物化能力。
2. 支持按资源类型决定物化单位：单文件、目录 subtree、skill 资源组、可写目录工作副本。
3. 在 `shell_exec.command`、relay MCP tool arguments 和显式 URL 转换入口中，把 VFS URI 自动重写为本机 path、目录 path 或短期 URL。
4. 为会被本机命令修改的目录提供稳定持久化方案，例如 `uv init skill-assets://skills/foo` 这类场景。
5. 保持 Agent 可见语义稳定：Agent 仍引用 VFS URI，默认不暴露 materialized local path。
6. 为开发者提供 debug audit / trace，记录 URI、provider、local backend、cache hit、digest、rewrite 映射和失败原因。

## Non-Goals

- 不做 VFS 写回云端。
- 不把 materialized path 当作 VFS 写入口。
- 不把 SkillAsset 或 lifecycle 资源同步到项目 workspace；只写 session scoped local cache。
- 不自动把可写工作副本发布回 SkillAsset / lifecycle；后续若需要回写，必须走独立的显式 publish/import 能力。
- 不实现 FUSE、symlink、watch、chmod 或完整 POSIX 文件系统。
- 不做 shell AST 静态分析；只识别明确的 session mount URI。
- 不把 materialized local path 注入给 Agent 作为稳定上下文或 prompt 内容。

## 当前链路 Review

### Application / Cloud 侧

- `RelayRuntimeToolProvider` 构建 runtime tools 时能拿到当前 session 的 tool build context，其中包含 `context.session.vfs`、`context.session.turn_id`、`context.session.identity`。
- `ShellExecTool` 目前只保存 `RelayVfsService` 和 `SharedRuntimeVfs`，没有保存 `session_id` / `turn_id` / identity，因此实现物化前需要扩展构造参数。
- `RelayVfsService::read_text()` 已经能按 `Vfs` + `ResourceRef` + provider registry 读取 `skill_asset_fs`、`lifecycle_vfs`、`inline_fs`、`relay_fs` 等 provider。
- `RelayVfsService::exec()` 只校验 exec mount 的 `Exec` capability 和 `cwd`，不处理 `command` 内的 URI。

### Relay FS Provider

- `RelayFsMountProvider::exec()` 将 exec 转发给 `mount.backend_id` 对应的 local backend。
- 现有 `ToolShellExecPayload` 没有 session id、turn id、物化文件列表或 rewrite metadata。
- `mount_root_ref` 是 local shell 的安全边界和默认工作目录，不是通用资源 cache。

### Local Backend

- `ToolExecutor` 负责本机 path 安全检查和 shell 执行。
- `resolve_shell_cwd()` 允许绝对 `cwd`，但必须仍位于 `mount_root_ref` 内；命令参数中的绝对路径没有额外校验，交给 shell 本身执行。
- local 当前没有 VFS provider registry，也不能直接读取云端 `skill_asset_fs` / `lifecycle_vfs`。
- local MCP stdio / HTTP client 由 `McpClientManager` 管理；`call_tool()` 只把 JSON arguments 原样传给 MCP server。

结论：物化职责不能只放在 application 侧写 cloud temp file。正确分工是 application 负责识别、授权和读取 VFS 资源；relay 负责把资源内容和物化请求传到目标 local backend；local 负责写入本机 session cache，并把 local path / localhost URL 返回给 application 用于 rewrite。

## 目标分工

### 1. Application: URI 解析、授权、读取、重写编排

新增 application 层编排服务，建议位置：

```text
crates/agentdash-application/src/vfs/materialization.rs
crates/agentdash-application/src/vfs/rewrite.rs
```

职责：

- 扫描 `shell_exec.command`、relay MCP tool arguments 中的字符串值，以及显式 URL 转换请求。
- 只识别当前 session `Vfs.mounts[].id` 对应的 `mount_id://relative/path`。
- 通过 `parse_mount_uri()` 跑 link resolution，并通过 `resolve_mount(..., Read)` 校验资源可读。
- 对 `relay_fs` 且 `source.backend_id == exec.backend_id` 的资源，直接重写成同一台 local backend 上的 workspace path，不复制内容。
- 对 `skill_asset_fs`、`lifecycle_vfs`、`inline_fs`、`canvas_fs` 或跨 backend 的 `relay_fs` 资源，先生成 `MaterializationPlan`，再通过 provider 读取计划内资源并请求目标 local backend 物化。
- 根据返回的 `local_path` / `local_url` 做平台安全 quoting 和字符串替换。
- 失败时中止整个工具调用，返回清晰错误，不做半成功执行。

### 2. Relay Protocol: 传输物化请求与响应

在 `crates/agentdash-relay/src/protocol.rs` 增加一组 local materialization 命令：

```text
command.vfs.materialize
response.vfs.materialize
command.vfs.materialize_cleanup
response.vfs.materialize_cleanup
```

第一版 payload 以文本资源为主；binary / streaming 通过 policy 显式开启。payload 必须支持一次下发多个 entry，避免 skill 脚本只物化单文件后丢失相邻 helper、references 或 assets。

建议字段：

```rust
pub struct VfsMaterializePayload {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub plan_id: String,
    pub plan_kind: MaterializationPlanKind,
    pub source_uri: String,
    pub root_uri: String,
    pub mount_id: String,
    pub provider: String,
    pub primary_relative_path: String,
    pub target_kind: MaterializationTargetKind,
    pub access_mode: MaterializationAccessMode,
    pub entries: Vec<VfsMaterializeEntry>,
    pub cache_scope: MaterializationCacheScope,
    pub ttl_ms: Option<u64>,
}

pub enum MaterializationPlanKind {
    SingleFile,
    DirectorySubtree,
    SkillResourceSet,
    WritableWorkingCopy,
}

pub enum MaterializationTargetKind {
    File,
    Directory,
}

pub enum MaterializationAccessMode {
    ReadOnly,
    WritableLocalCopy,
}

pub struct VfsMaterializeEntry {
    pub relative_path: String,
    pub content: VfsMaterializeContent,
    pub digest: String,
    pub size_bytes: u64,
    pub mime_hint: Option<String>,
    pub executable_hint: bool,
}

pub enum VfsMaterializeContent {
    Utf8Text { text: String },
    Base64Bytes { data: String },
}

pub struct VfsMaterializeResponse {
    pub source_uri: String,
    pub local_root_path: String,
    pub primary_local_path: String,
    pub primary_local_url: Option<String>,
    pub access_mode: MaterializationAccessMode,
    pub manifest_digest: String,
    pub total_size_bytes: u64,
    pub entry_count: usize,
    pub dirty: bool,
    pub cache_hit: bool,
}
```

relay 不理解 VFS 语义，只保证消息可靠送到指定 `backend_id`。

### 3. Local Backend: 本机 cache、权限、localhost URL

新增 local 侧存储模块，建议位置：

```text
crates/agentdash-local/src/materialization.rs
crates/agentdash-local/src/handlers/materialization.rs
crates/agentdash-local/src/resource_server.rs
```

职责：

- 对只读执行场景，写入 session scoped cache，例如：

```text
{temp}/agentdash/materialized/{backend_id}/{session_id}/
  manifest.json
  objects/
    sha256-{digest}/
      {sanitized-file-name}
```

- 对可写目录工作副本，写入 stable local data root，而不是 temp，例如：

```text
{local_data}/agentdash/materialized-workdirs/{backend_id}/
  {resource_key}/
    manifest.json
    content/
      ...
```

- 只接受 relay 下发的相对 resource metadata，不信任 payload 中的本机路径。
- 校验 `size_bytes`、`digest`、`max_bytes`、content encoding 和 path segment。
- 写入后尽量设置只读；`executable_hint=true` 时 Unix 设置用户执行位，Windows 不做 chmod。
- 返回本机 absolute path；如果 URL policy 要求，返回绑定该 cache object 的 `http://127.0.0.1:{port}/...` 短期 URL。
- session terminal / delete / backend disconnect 时按 session cache 清理；清理失败只记录 warning。
- persistent working copy 不随 session 自动删除；由 manifest 记录来源和 last_used，由后续维护任务做 TTL / quota 清理。

`ToolExecutor` 继续只执行本机 shell，不承担 VFS 读取。它只消费 application 已经 rewrite 好的 command。

### 4. 物化单位与资源组策略

新增 `MaterializationPlan`，不要把所有 URI 都简单等价成单文件：

```rust
pub struct MaterializationPlan {
    pub kind: MaterializationPlanKind,
    pub source_uri: String,
    pub root_uri: String,
    pub primary_relative_path: String,
    pub entries: Vec<MaterializationEntryRef>,
    pub target_kind: MaterializationTargetKind,
    pub access_mode: MaterializationAccessMode,
    pub working_dir_hint: Option<String>,
}
```

默认策略：

- 普通文本资源：`SingleFile`，只物化 URI 指向的文件。
- 明确目录 URI 或 URL preview 目录场景：`DirectorySubtree`，按 policy 限制递归深度、总大小、文件数量。
- Skill 脚本：`SkillResourceSet`，当 URI 匹配 `skills/{skill_key}/scripts/...` 时，物化该 skill 的可运行上下文。
- 明确目录作为 shell/MCP 参数出现时：`DirectorySubtree` 或 `WritableWorkingCopy`，rewrite 结果必须是目录本身，而不是目录下某个文件。

目录 URI 判定：

- URI 以 `/` 结尾、provider `stat/list` 判断为目录，或路径正好匹配 skill root `skills/{skill_key}` 时，target kind 为 `Directory`。
- shell command 中的目录 URI 作为命令参数出现时，第一版默认生成 `WritableLocalCopy`，因为命令可能执行 `uv init`、`npm install`、生成缓存或写配置。
- URL preview 或纯读取 API 可显式要求 `ReadOnly`。
- 对同一个 root URI，后续 turn / session 再次 materialize 应返回同一个 stable working copy path，除非 manifest 检测到 source digest 已变化且本地 copy 有未发布修改。

Skill 脚本策略第一版建议：

- `root_uri` 为 `skill-assets://skills/{skill_key}` 或 `lifecycle://skills/{skill_key}`。
- `primary_relative_path` 为被执行脚本相对 skill root 的路径，例如 `scripts/check.sh`。
- 默认 entry 包含：
  - `SKILL.md`
  - `scripts/**`
  - `references/**`
  - `assets/**`
- 保留 skill root 下的相对目录结构，让脚本可以通过自身路径定位相邻资源。
- command rewrite 只替换原始脚本 URI 为 `primary_local_path`；`cwd` 仍保持 Agent 指定的 exec mount 目录。
- 可选提供 `AGENTDASH_MATERIALIZED_ROOT` / `AGENTDASH_SKILL_ROOT` 环境变量，但第一版不依赖 Agent 感知这些变量。
- 若 entry 数量、总大小或 binary policy 超限，整个物化失败并阻止执行。

这个策略同样适用于 lifecycle 暴露的 skill projection：只要路径形态是 `lifecycle://skills/{skill_key}/scripts/...`，就按 skill root 物化相关上下文，而不是只读单个投影文件。

可写目录工作副本策略：

- `resource_key` 由 `target_backend_id`、provider、mount id、mount root_ref、root URI、identity scope 共同 hash 得到。
- `manifest.json` 至少记录 `source_uri/root_uri`、source manifest digest、entry list、created_at、last_used、dirty marker、access mode。
- 初次创建时从 VFS provider 拉取目录 subtree 到 `content/`。
- 后续同一资源复用 `content/`，保证 shell 命令看到稳定路径。
- 如果本地 `content/` 已 dirty 且云端 source digest 变化，默认失败并记录 conflict；不做静默覆盖。
- 如果本地未 dirty 且 source digest 变化，可以安全刷新工作副本。
- 第一版不自动回写 VFS；要把工作副本变成 SkillAsset 更新，必须另走显式 publish/import 能力。

### 5. Rewrite 接入点

#### `shell_exec.command`

执行顺序：

1. `ShellExecTool` 从 tool build context 保存 `session_id`、`turn_id`、identity。
2. 解析 `cwd` 得到 exec mount，并确认 exec mount provider 是可执行的 `relay_fs`。
3. 扫描 `params.command` 中的 VFS URI。
4. 对每个 URI 生成 `MaterializationRequest`，目标 backend 是 exec mount 的 `backend_id`。
5. 调用 local materialize，拿到 `primary_local_path` 和可选 `local_root_path`。
6. 按当前 local OS shell quoting 规则替换命令字符串。
7. 将 rewrite 后的 command 交给现有 `RelayVfsService::exec()`。

注意：Windows 当前 `ToolExecutor` 使用 `cmd /C`，Unix 使用 `sh -c`，quoting 必须按这两个 shell 分开实现。

#### Relay MCP tool arguments

relay MCP server 运行在 local backend 上，MCP arguments 里的 VFS URI 也需要同样处理。

接入点：

- `crates/agentdash-application/src/runtime_gateway/session_actions.rs` 或 relay MCP tool adapter 层，在 `BackendRegistry::call_relay_tool()` 前处理 JSON arguments。
- 扫描 JSON string leaf，其他类型保持不变。
- URL-like 字段可按 policy 转为 local URL；path-like 字段默认转为 local path；目录 URI 必须转为目录 root path。
- rewrite metadata 只进入 debug audit / trace，不改写 Agent 可见工具 schema。

第一版可以只支持 tool call arguments，不处理 local MCP server 启动配置里的 command / args / env；启动配置物化可作为后续明确需求。

#### URL 转换

提供显式转换 API，而不是把所有文本里的 URI 自动深度改写：

```text
POST /api/sessions/{session_id}/vfs-materializations/url
```

输入 `source_uri`、`target_backend_id`、`policy`，输出短期 URL。URL 的提供者按消费场景选择：

- 本机消费者：通过 relay 请求 local backend 物化并返回 `127.0.0.1` URL。
- Dashboard / cloud UI 消费者：可以由 cloud API endpoint 直接 stream provider 内容，但必须复用同一套权限、token 和 audit 结构。

HTML / CSS / Markdown 内相对资源递归改写不在第一版自动做；只提供显式 URI 到 URL 的转换能力。

## 完整信息契约

一次物化至少需要以下信息：

### Session 与执行目标

- `session_id`：cache namespace 和权限边界。
- `turn_id`：debug / trace 归因。
- `tool_call_id`：关联到 shell/MCP 调用。
- `rewrite_source`：`shell_exec.command`、`mcp.arguments`、`url_proxy` 等来源。
- `target_backend_id`：资源要落到哪台 local backend。
- `exec_mount_id` / `exec_mount_root_ref`：shell 执行 mount 与 workspace root；同 backend `relay_fs` 直连 path rewrite 要用它。
- `identity`：传给 provider 的当前用户身份。

### VFS 与源资源

- session `Vfs` snapshot：`mounts`、`default_mount_id`、`links`。
- link resolution 后的 `ResourceRef { mount_id, path }`。
- source mount 的 `provider`、`backend_id`、`root_ref`、`capabilities`、`metadata`。
- `source_uri`：原始 URI，用于 audit 和 manifest。
- `resolved_uri`：link resolution 后的 URI，用于真实读取。

### 物化计划

- `plan_kind`：`SingleFile` / `DirectorySubtree` / `SkillResourceSet`。
- `root_uri`：本次物化的资源根；单文件时可等于 `source_uri`。
- `primary_relative_path`：原 URI 对应的主入口文件，rewrite 应指向它。
- `entries`：要下发给 local 的文件列表，每个 entry 有相对 root 的路径、digest、size、mime、执行提示。
- `target_kind`：File 或 Directory；决定 rewrite 指向主文件还是资源组根目录。
- `access_mode`：ReadOnly 或 WritableLocalCopy；决定 local 落盘位置、文件权限、cleanup 策略。
- `limits`：最大文件数、最大总字节数、最大递归深度、是否允许 binary。
- `working_dir_hint`：仅用于 debug 和后续策略扩展；第一版不改变 shell cwd。

### 内容与缓存

- `content`：每个 entry 的 UTF-8 text 或显式允许的 bytes。
- `digest`：每个 entry 的 `sha256`，由 application 按实际内容计算，local 写入后复验。
- `manifest_digest`：整个资源组 manifest 的 digest，用于 cache hit。
- `size_bytes`：每个 entry 与总大小限制。
- `mime_hint` / `file_kind`：URL content type、binary policy、script hint。
- `executable_hint`：脚本类 entry 可请求设置执行权限。
- `cache_scope`：`Turn`、`Session` 或 `PersistentWorkingCopy`；文件执行默认 `Session`，可写目录默认 `PersistentWorkingCopy`。
- `ttl_ms`：URL token 或临时 cache 过期时间。
- `resource_key`：stable working copy 的本机 key；只进入 local manifest 和 debug trace，不暴露给 Agent。

### Rewrite 与观测

- `matches`：原字符串中的 match span、source URI、replacement kind。
- `replacement`：文件 URI 用 `primary_local_path` / `primary_local_url`，目录 URI 用 `local_root_path`。
- `local_root_path`：资源组根路径，只进入 audit / trace，不默认暴露给 Agent。
- `access_mode` 与 `dirty`：用于区分只读 cache、可写工作副本和冲突状态。
- `shell_flavor`：`cmd` 或 `sh`，决定 quoting。
- `cache_hit`：是否复用已有物化对象。
- `failure_policy`：任一资源失败则整个工具调用失败。
- `audit_event`：记录 source URI、resolved URI、provider、backend、digest、size、local path/URL token、耗时和错误。

## Implementation Plan

### Phase 1：协议与 local cache 基座

- 在 relay protocol 增加 `vfs.materialize` / cleanup 命令和响应。
- 新增 local `MaterializationStore`，实现安全 cache root、stable workdir root、manifest、digest 复验、只读写入、session cleanup。
- 增加 local handler，把 materialize payload 落盘并返回 path / URL。
- 单测覆盖 path sanitization、digest mismatch、size mismatch、cache hit、cleanup、persistent working copy reuse、dirty conflict。

### Phase 2：Application 编排服务

- 新增 `MaterializationCoordinator`，封装 scan、VFS resolve、provider read、relay local materialize。
- 新增 `MaterializationPlanner`，按 provider/path/policy 生成单文件、目录资源组或可写工作副本计划。
- 扩展 `RelayRuntimeToolProvider` 构造 `ShellExecTool` 时传入 `session_id`、`turn_id`、identity 和 coordinator。
- 实现 `relay_fs` same-backend direct path rewrite；非本机 provider 走 local materialization。
- 单测覆盖 `skill-assets://`、`lifecycle://`、skill script resource set、目录 URI rewrite、unknown mount、capability denied、same-backend relay_fs direct path。

### Phase 3：`shell_exec.command` 自动重写

- 在 `ShellExecTool::execute()` 调用 `RelayVfsService::exec()` 前重写 command。
- 按 `cmd` / `sh` 分别实现 quoting。
- 物化失败时不执行 shell。
- Debug audit / trace 记录 rewrite summary。
- 测试覆盖多个 URI、空格、中文、引号、Windows 路径、未知 scheme 不改写。

### Phase 4：Relay MCP arguments 与 URL 转换

- 在 relay MCP call 进入 local backend 前扫描 JSON string leaf 并物化。
- 新增显式 VFS URI to URL API。
- local resource server 提供短期 `127.0.0.1` URL；cloud API 可为 dashboard 复用同一 token 模型。
- 测试覆盖 MCP argument rewrite、URL token session binding、过期、权限失败、binary policy。

### Phase 5：清理与观测

- 接入 session terminal / delete / backend disconnect 的 cache cleanup。
- 统一 debug audit 事件结构。
- 在 lifecycle VFS 的 tool-call projection 中可通过现有 session events 看到 rewrite 归因，但不向 Agent prompt 注入 local path。

## Acceptance Criteria

- [ ] `shell_exec` 执行 `bash lifecycle://skills/<key>/scripts/check.sh` 前，会把脚本物化到目标 local backend 并把 command 改写为本机 path。
- [ ] Skill 脚本物化时保留 `SKILL.md`、`scripts/**`、`references/**`、`assets/**` 的相对布局，脚本可通过自身路径访问关联资源。
- [ ] `uv init skill-assets://skills/<key>` 这类目录参数会被 rewrite 到 stable writable local working copy root。
- [ ] 同一资源目录跨 turn / session 重复物化能复用 stable working copy；source 变化和本地 dirty 冲突不会被静默覆盖。
- [ ] `shell_exec.command` 中多个 VFS URI 会分别物化并替换；任一失败则命令不执行。
- [ ] `main://path` 指向同一 local backend 的 `relay_fs` 资源时直接改写为 workspace 内 path，不复制内容。
- [ ] `skill-assets://skills/<key>/references/rules.md` 可物化为本机只读 cache 文件。
- [ ] relay MCP tool arguments 中的 VFS URI 能在调用 local MCP server 前被改写。
- [ ] URL-only 消费者能把允许的 VFS URI 转为短期 URL；token 绑定 session 和目标 backend。
- [ ] 未知 scheme、`http://`、`https://`、`data:` 不被误写。
- [ ] path traversal、绝对路径、超限文件、无 Read capability mount、backend offline 都失败且错误清晰。
- [ ] 同一 session 内相同内容重复请求复用 cache，manifest 可解释来源。
- [ ] Session terminal / delete / backend disconnect 后 local cache 被清理；清理失败只记录 warning。
- [ ] Debug audit / trace 中能看到 source URI、resolved URI、provider、target backend、local path/URL token、digest、size、cache hit、rewrite source。

## Verification

```bash
cargo test -p agentdash-relay vfs_materialize
cargo test -p agentdash-local materialization
cargo test -p agentdash-application vfs::materialization
cargo test -p agentdash-application vfs::tools::fs
cargo test -p agentdash-api relay_fs
cargo test -p agentdash-api mcp_relay
```

若实现包含前端 URL 使用入口：

```bash
pnpm --filter frontend exec tsc --noEmit
```

## Related Specs

- `.trellis/spec/backend/vfs/vfs-access.md`
- `.trellis/spec/backend/embedded-skill-bundles.md`

## Open Questions

- Provider 层是否先新增 `read_binary`，还是第一版只支持 UTF-8 text，binary 仅在 URL phase 处理？
- local resource server 的端口和生命周期由 local backend 自主管理，还是注册到 cloud API 统一分发？
- persistent working copy 的默认 TTL / quota / 手动清理入口如何设计？
- 可写工作副本后续是否需要显式 publish/import 到 SkillAsset；如果需要，应另建独立任务，不混进自动物化主线。
- MCP argument rewrite 如何判断 path-like / url-like 字段：第一版建议 string leaf 全量扫描，replacement kind 可由 caller policy 指定。
- Windows 是否需要支持 PowerShell quoting，还是先严格对齐当前 `cmd /C` 执行器？
