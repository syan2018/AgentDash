# 统一资源寻址与 VFS 路径收敛

## Goal

结合 `docs/reviews/AgentDash_review_report.md` 与 2026-05-16 最新进展，把仍有效的跨层重构收敛为一个可追踪任务：统一资源地址模型、路径策略、VFS/Relay/local 边界、Lifecycle catalog、API/前端契约与文档权威来源。

## Review 结论

这份评估的主判断仍然成立：当前最大系统性风险不是单点 bug，而是同一个资源地址在 VFS、Relay、本机执行、Lifecycle、API、前端里被多处、多种方式解释。路径、mount root、URI、working directory 和本机路径仍以 `String` / `PathBuf` 在层间穿行，导致权限边界、审计语义和错误模型容易分叉。

需要按最新进展修正的部分：

- SessionHub 已经不是本轮主风险。`05-16-session-refactor-cleanup` 已记录 `SessionHub` 符号删除、`SessionRuntimeBuilder` / `SessionRuntimeServices` 对外收口、terminal effect 与 runtime command 去 pending 命名等完成项。因此报告里以 SessionHub 为中心的措辞应视为历史上下文，不再作为本 task 的实施重点。
- `SessionLaunchPlanner` 已不再使用旧的 `resolve_working_dir` 直接处理用户 working_dir；当前更准确的问题是它从 default mount 的 `root_ref` 直接构造 `PathBuf` 作为执行工作目录。这个问题仍属于资源地址模型未类型化，而不是单个 helper 函数未修。
- `04-08-cross-mount-shell-materialization` 已覆盖 VFS URI 物化和 shell/MCP rewrite，它应作为相关任务协同推进；本 task 不重复实现物化功能，而负责定义地址类型、路径策略和 relay/local 边界，使物化任务有稳定底座。
- 报告中“先 warn 再 hard error”的过渡策略不适合当前预研项目约束。本项目未上线，不需要兼容旧 API/字段/路径；重构应直接落到最正确的状态，必要时配套 migration 和测试，不保留旧行为回退。

## Requirements

- 建立统一资源寻址模型，至少覆盖 `MountId`、`MountRelativePath`、`VfsUri`、`RootRef`、`PathPolicy`，并明确原始字符串只能存在于 API/UI/relay 边界。
- `parse_mount_uri`、`normalize_mount_relative_path`、link resolution、provider dispatch、relay payload、local path 解析必须形成单一职责链路；禁止 provider 或 local runtime 重新猜测已经验证过的路径语义。
- `Mount.root_ref` 必须从裸字符串语义收敛为可区分的 local root 与 provider URI。虚拟 root 不得被隐式转成 OS `PathBuf`。
- Session 执行工作目录必须表达为受 VFS/default mount 约束的地址，禁止绝对路径覆盖、越界 `..` 和虚拟 mount 到 OS path 的隐式转换。
- `apply_patch_multi` 必须同时规范化 primary path 与 `move_path`；跨 mount move 必须明确禁止或以显式事务化 copy/delete 实现，不能半隐式执行。
- Relay search、local search/list、shell cwd、file read/write 的路径策略必须集中为 `PathPolicy`；搜索结果不能 fallback 暴露本机绝对路径。
- VFS mount 必须在构建后执行 hard validation：mount id 唯一、reserved id 冲突、default mount 存在、provider/root_ref 合法、link 无环、capability 与 provider 支持一致。
- Lifecycle VFS 目录必须由单一 `LifecyclePathCatalog` 或等价模型生成 metadata、list/read/write/alias，消除 `active/*`、`session/*`、`nodes/{step_key}/*` 的手写漂移。
- API/前端契约必须收敛：VFS/file-picker 不再作为两套资源 API 并存，前端不在页面/组件中手写后端 endpoint 字符串。
- README/docs 需要标记当前权威、草案、历史归档；已被最新 session 重构淘汰的 `SessionHub` 表述要清理或移动到历史背景。

## Acceptance Criteria

- [x] VFS 地址 newtype 与 `PathPolicy` 已落地，关键入口不再直接传裸 `String` 作为已验证路径。
- [x] `RootRef` 能区分本机路径和 provider URI；虚拟 root 不再被隐式 `PathBuf::from` 后传入 connector/session 工作目录。
- [x] session launch、relay exec、local file/search/shell cwd 共用统一路径策略测试矩阵，覆盖 absolute、UNC、Windows drive、`..`、重复 slash、URI prefix、link loop。
- [x] `apply_patch_multi` 覆盖 same mount move、cross mount move、absolute move target、escaping move target 的表格测试。
- [x] Relay search 保持 `{ mount_root_ref, path }` 边界，不再把 search base 拼进 root_ref。
- [x] local search/list/fallback search 无法 strip workspace root 时返回错误或跳过，不返回绝对路径 fallback。
- [x] `Vfs::validate()` 或等价验证在 VFS 构建/合并后执行，违规直接失败。
- [x] Lifecycle catalog 成为目录 schema 的单一来源，metadata 与 provider 行为不再重复手写。
- [x] VFS/file-picker 前端调用面收敛到同一 route manifest，前端 service 不再散拼高频 endpoint。
- [x] 文档和 review 报告中的历史 `SessionHub` 目标态已按最新实现标注为非本 task 范围。

## Constraints

- 本项目仍处预研期，不做兼容层、不保留旧行为回退、不引入 warn-only 过渡。
- 如需调整 API、DTO、数据库 schema，直接做正确迁移并更新调用方。
- 现有 `05-16-session-refactor-cleanup` 与 `04-08-cross-mount-shell-materialization` 是相关任务，不在本 task 中重复它们的已完成/专属范围。
