# 统一资源寻址与 VFS 路径收敛 Implement

## Phase 0：Review 基线

- [x] 读取 `docs/reviews/AgentDash_review_report.md`。
- [x] 对齐最新 session 进展：`05-16-session-refactor-cleanup` 已覆盖 SessionHub 删除、terminal effect/runtime command 收尾。
- [x] 确认 `04-08-cross-mount-shell-materialization` 与本 task 的边界：物化任务负责 VFS URI 到本机 path/URL，本 task 负责地址模型与路径策略底座。
- [x] 把“预研期不做兼容/回退”的约束纳入任务范围。

## Phase 1：路径安全 P0

- [x] 为 `MountRelativePath`、`VfsUri`、`RootRef`、`PathPolicy` 补最小类型和测试。
- [x] 修正 session launch working directory：不再从 virtual/default `root_ref` 直接构造 OS `PathBuf`。
- [x] 修正 `apply_patch_multi`：primary path 与 `move_path` 同时解析、规范化、分 mount；禁止跨 mount move。
- [x] 修正 relay search：search base path 不拼入 `mount_root_ref`。
- [x] 修正 local search/list/fallback：无法 strip workspace root 时不返回绝对路径。
- [x] 增加路径安全测试矩阵：absolute、UNC、Windows drive、`..`、重复 slash、URI prefix、link loop、cross-mount patch。

## Phase 2：VFS 验证与边界收敛

- [x] 增加 `Vfs::validate()` 等价验证入口 `validate_vfs`。
- [x] 在 `build_derived_vfs`、workspace VFS、agent knowledge VFS 构建后执行 hard validation。
- [x] 补齐 `validate_vfs` 对 default mount 必填、系统保留 mount id 与内置 provider capability 的硬校验。
- [x] 收敛 `RootRef` provider/local 语义，关键入口不再把虚拟 root 当作本机 path。
- [x] 把 relay/local file、list、search、shell cwd 策略集中到共享路径解析 helper。
- [x] 为 materialization 任务暴露稳定地址解析接口，避免其重复实现 URI/path 判断。

## Phase 3：Lifecycle Catalog

- [x] 设计 `LifecyclePathCatalog` canonical schema。
- [x] 由 catalog 生成 directory hints 和 root/active list entries。
- [x] 将 provider 根目录与 active 目录 list 迁移到 catalog-driven dispatch。
- [x] 增加 catalog 与 lifecycle_vfs 回归测试。

## Phase 4：API / Frontend 契约

- [x] 前端 VFS/file-picker service 共用同一 route manifest。
- [x] 建立 `vfsRoutes` route manifest。
- [x] 前端 VFS/file-picker 相关 service 改为集中 route manifest。
- [x] 高频 VFS/file-picker endpoint 字符串只保留在 route manifest。

## Phase 5：Docs 权威化

- [x] 将本任务中固化的新地址模型写入 `.trellis/spec/backend/vfs/vfs-access.md`。
- [x] 在任务记录中标注 `SessionHub` 已由 `05-16-session-refactor-cleanup` 消化，不再作为本任务范围。
- [x] README/docs 分层清理保留为后续专门文档卫生任务，不阻塞本轮路径/VFS 重构完成。

## Validation

```bash
cargo test -p agentdash-application vfs --lib
cargo test -p agentdash-application session --lib
cargo test -p agentdash-api relay_fs
cargo test -p agentdash-local tool_executor
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
pnpm --filter @agentdash/app-web exec tsc --noEmit
```

## Validation Result

- [x] `cargo test -p agentdash-application vfs::path --lib`
- [x] `cargo test -p agentdash-application patch_entry --lib`
- [x] `cargo test -p agentdash-application session_working_directory --lib`
- [x] `cargo test -p agentdash-local resolve_shell_cwd --lib`
- [x] `cargo test -p agentdash-application lifecycle_catalog --lib`
- [x] `cargo test -p agentdash-application lifecycle_vfs --lib`
- [x] `cargo check -p agentdash-application`
- [x] `cargo check -p agentdash-api`
- [x] `cargo check -p agentdash-local`
- [x] `pnpm --filter app-web run typecheck`
