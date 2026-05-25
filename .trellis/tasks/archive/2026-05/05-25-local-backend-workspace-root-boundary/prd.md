# 本机后端 workspace root 授权边界重构

## Goal

梳理并重构本机后端 workspace root 与 ProjectBackendAccess / Workspace Inventory / ToolExecutor path policy 的职责边界，修复用户已经通过本机目录选择和 Project 授权流程确认目录后，后续 `workspace.detect`、session prompt 或文件工具仍被启动期 `accessible_roots` 拒绝的问题。

本任务的目标心智是：本机后端默认允许用户浏览本机目录以选择 workspace；被用户选中并登记的 workspace root 是后续执行边界的事实；空的 workspace roots 不应变成阻断、警告或要求用户处理的状态。

## Confirmed Facts

- 当前 `workspace.browse_directory` 可以浏览 Windows 盘符和任意本机目录，不受 `accessible_roots` 限制。
- 当前 `workspace.detect` 在本机侧会调用 `ToolExecutor::validate_workspace_root`，要求待检测路径落在启动期 `accessible_roots` 内。
- 当前 ProjectBackendAccess / Workspace Inventory 登记链路不会扩大 local runtime 的 `accessible_roots`，导致“能浏览选择目录，但 detect/register 被拒绝”的流程断裂。
- 当前 `accessible_roots` 同时承担注册能力上报、ToolExecutor 安全边界、session runtime 默认根、SQLite 本机库目录和 MCP 配置根等职责。
- 当前 `ToolExecutor::validate_workspace_root` 对空 `accessible_roots` 的语义是允许任意路径；但 inventory refresh、runtime 日志和 session runtime 兜底又把空 roots 表达成“没有 roots / 使用当前目录兜底”。
- 用户明确要求：本机通常默认全盘允许查看；当没有 `workspace_roots` 时不要让该状态刷存在感。

## Requirements

- 本机目录浏览必须继续支持默认全盘浏览，用于让用户选择候选 workspace 目录。
- `workspace.detect` / `workspace.detect_git` 必须以“用户选择的本机目录存在且可访问”为前置条件，不得再要求该目录预先位于启动期 `accessible_roots`。
- ProjectBackendAccess 的 inventory register 流程必须支持登记用户刚选择的本机目录，不得出现“授权目录不在 accessible_roots 内”的错误。
- 后续 session prompt、file tool、shell、apply_patch 等执行能力仍必须有明确 workspace root 边界；该边界以当前 session 的 `mount_root_ref` / 已登记 workspace binding 为事实源。
- 当存在显式 `workspace_roots` 时，执行类能力可以用它作为可执行 workspace 集合；当不存在显式 `workspace_roots` 时，不应把空集合当作拒绝所有目录或要求用户维护 roots 的信号。
- `accessible_roots` 的产品语义需要移除：它不应再作为用户授权目录的启动期事实源。涉及本机已确认工作目录集合的地方统一朝 `workspace_roots` 语义收敛。
- 本机 runtime 默认工作目录、SQLite 本机库目录和 MCP 配置根不能继续隐式依赖 `accessible_roots.first()` 作为长期模型；本任务至少要明确拆分方案并落地必要的第一阶段代码调整。
- UI / API 错误文案不得把空 workspace roots 暴露成需要用户修复的问题；只有具体目录不存在、不是目录、不可读取或执行越界时才返回可操作错误。
- 更新跨层 spec，记录 workspace root 事实源、目录浏览默认行为、detect/register 与执行边界的关系。

## Acceptance Criteria

- [ ] 用户通过本机目录浏览选择 `D:\ProjectABC_Dev\yihao.liao_ABC_Project_0.65dev` 后，ProjectBackendAccess inventory register 可以完成 detect 并登记，不再因启动期 `accessible_roots` 拒绝。
- [ ] 在没有显式 workspace roots 的本机 runtime 中，目录浏览和 workspace detect 不展示或返回 “未配置 accessible_roots” 类错误。
- [ ] 执行类文件和 shell 操作仍被限制在 session 的 `mount_root_ref` 内，`..` 逃逸、绝对 `cwd` 越界和 symlink 越界继续被测试覆盖。
- [ ] 如果存在显式 workspace roots，执行类能力对不属于该集合的 `mount_root_ref` 有清晰、可排障的错误；错误应包含目标路径语义，不应要求用户理解旧的 `accessible_roots` 概念。
- [ ] `workspace.detect` / `workspace.detect_git`、ProjectBackendAccess inventory register、session prompt、relay fs tools 至少有覆盖核心路径的单元测试或集成测试。
- [ ] `.trellis/spec/cross-layer/project-backend-workspace-routing.md` 和相关本机 runtime spec 已更新为新的职责边界。
- [ ] 相关 Rust 测试通过，前端类型检查或受影响测试通过。

## Out Of Scope

- 不保留旧的启动期目录白名单作为产品兼容路径。
- 不引入数据库字段兼容方案；如果 schema 需要调整，按当前预研阶段直接迁移到正确模型。
- 不重做完整本机 runtime profile UI，只调整与 workspace root 语义和错误展示直接相关的控件、文案和数据流。
- 不把云端变成本机文件系统事实源；本机仍负责真实路径检测和文件操作。

## Open Questions

- 无。默认按预研项目策略推进到正确模型：移除 `accessible_roots` 作为产品概念，字段、文档和用户可见文案统一朝 `workspace_roots` / session `mount_root_ref` 边界收敛。
