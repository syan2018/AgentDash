# 统一资源寻址与 VFS 路径收敛 Design

## Target Architecture

目标是把资源地址解释权收口到 application 层的地址解析和策略模块，再把已验证的结构化地址传给 provider、relay、local runtime 与前端 client。

```text
UI/API/Relay raw input
  -> VfsUri::parse + MountRelativePath::normalize
  -> Vfs::validate + PathPolicy
  -> ResourceAddress / RootRef
  -> Provider dispatch / Relay payload / Local path resolver
```

任何层如果收到的是已验证地址，就不再重新解析裸字符串；任何层如果收到 raw input，必须显式标注边界并立即 parse/normalize。

## Workstreams

### 1. Address Types

新增或收敛以下类型：

- `MountId`
- `ProviderId`
- `BackendId`
- `MountRelativePath`
- `VfsUri`
- `RootRef::LocalPath | RootRef::ProviderUri`
- `PathPolicy::{VfsRead,VfsWrite,VfsList,VfsSearchBase,PatchTarget,PatchMoveTarget,ShellCwd,SessionWorkingDir,MaterializationTarget}`

这些类型先在 application/domain 关键链路使用，不为了兼容旧调用保留双写模型。

### 2. VFS Validation

为 `Vfs` 增加 hard validation：

- mount id 唯一；
- reserved id 不被用户 mount 占用；
- default mount 存在；
- provider 与 root_ref scheme 合法；
- link target 存在且无环；
- capability 与 provider 能力一致。

`build_derived_vfs`、session construction、capability overlay merge 后都必须验证。

### 3. Session Working Directory

Session launch 不再把 default mount `root_ref` 直接 `PathBuf::from` 后作为工作目录。目标模型：

- working dir 是 `SessionWorkingDir` 或 `VfsUri`；
- relay/local 执行前由 `PathPolicy::SessionWorkingDir` 解析；
- virtual mount 只能 materialize 后成为 local path；
- absolute path 和 escaping `..` 直接失败。

### 4. Relay / Local Path Boundary

Relay payload 保持 root 与 relative path 分离，尤其是 search：

```text
mount_root_ref: RootRef
path: MountRelativePath
```

local runtime 使用集中 `LocalPathPolicy` 解析 file/list/search/shell cwd。搜索/list 如果发现结果不在 workspace root 内，直接错误或跳过，不返回 absolute fallback。

### 5. Patch Coordinator

`apply_patch_multi` 要解析 primary path 与 `move_path` 的 mount prefix，并在同一处规范化。第一版策略禁止跨 mount move；后续如确需跨 mount，另做事务化 copy/delete 设计。

### 6. LifecyclePathCatalog

把 lifecycle 路径定义集中为 catalog：

- canonical path；
- alias/current path；
- list entries；
- read/write handler binding；
- metadata directory hints。

`active/*` 与 `nodes/{step_key}/*` 的关系必须以 alias 显式建模，而不是在多个 match arm 中重复。

### 7. API / Frontend Contract

VFS/file-picker 收敛为同一资源契约：

- 后端 route manifest 或生成式 client；
- 前端只通过集中 service/hook 调用 VFS；
- 页面和组件不得手写 endpoint 字符串；
- DTO 字段按后端 schema 生成或集中定义。

## Non-Goals

- 不重复实现 `04-08-cross-mount-shell-materialization` 的物化协议和 local cache；本 task 只提供地址、策略与边界底座。
- 不继续推进已由 `05-16-session-refactor-cleanup` 完成的 SessionHub 删除和 terminal effect 去 hub 化。
- 不做兼容旧 API/旧路径/旧字段的 fallback。

## Risks

- 地址 newtype 会触发多 crate 编译面，需要按工作流小步推进并保持每步测试可解释。
- Session working dir 与 relay exec 相连，任何失败语义调整都必须覆盖 hook auto-resume、local relay prompt、workflow/routine/companion 入口。
- Lifecycle catalog 改动容易影响 agent 可见路径，必须用 alias/canonical 等价测试保护。
