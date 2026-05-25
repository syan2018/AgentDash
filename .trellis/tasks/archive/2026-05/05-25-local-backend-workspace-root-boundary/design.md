# 本机后端 workspace root 授权边界重构设计

## Architecture Intent

当前问题来自同一个字段承担太多职责：`accessible_roots` 既像启动期白名单，又像本机 workspace inventory，又被 runtime 用作默认工作目录和本机持久化目录。新的设计把它拆成三个语义：

- 目录浏览能力：本机默认允许用户浏览全盘目录，用于选择候选 workspace。
- Workspace root 事实：用户选择并 detect/register 成功的目录，成为 ProjectBackendAccess / Workspace Inventory / Workspace Binding 的事实。
- 执行边界：session prompt 和工具调用只能在本次 session 的 `mount_root_ref` 内解析路径；可选的 workspace roots 只用于确认该 `mount_root_ref` 属于已登记 workspace 集合。

## Boundaries

### Local Runtime

- `workspace.browse_directory` 是选择器能力，不是执行能力；它可以全盘浏览，只返回目录项，不执行代码、不读取文件内容。
- `workspace.detect` / `workspace.detect_git` 是 setup 能力；它必须 canonicalize 目标路径，校验存在、是目录且可访问，然后读取必要的 Git / P4 facts。
- `ToolExecutor` 是执行能力；它负责把 session `mount_root_ref` 解析成 workspace root，并确保文件路径、写入 parent、shell cwd 和 apply_patch 都不逃出该 root。
- 当显式 workspace roots 为空时，`ToolExecutor` 不应把空集合作为阻断条件；此时 `mount_root_ref` 自身就是本次执行的边界。
- 当显式 workspace roots 非空时，`mount_root_ref` 必须位于其中一个已 canonicalize root 内。

### Cloud Backend

- Runtime Gateway 仍是 `workspace.detect`、`workspace.detect_git`、`workspace.browse_directory` 的唯一调用入口。
- ProjectBackendAccess inventory register 仍负责 Project 权限、access 状态和 inventory upsert，不直接访问本机文件系统。
- Workspace binding 继续表达 Project workspace 与 backend root 的绑定；它不表达 runtime 是否忙碌，也不表达目录浏览权限。

### Frontend / Desktop

- Local Runtime 设置页如果没有显式 workspace roots，不展示成异常状态。
- Project backend access 目录选择流程的心智是“选择本机目录并登记为 workspace root”，不是“先维护 accessible roots 再选择其中的目录”。
- 错误展示聚焦真实失败：目录不存在、不可读、不是目录、backend 离线、detect 失败、执行路径越界。

## Data Flow

### Browse

1. UI 调用 backend access browse 或 backend browse API。
2. Cloud 通过 Runtime Gateway 调用 local `command.browse_directory`。
3. Local 列出盘符或目录子目录并返回。
4. 不读取文件内容，不写入 workspace roots，不要求已有 roots。

### Detect / Register

1. UI 选择目录并提交 `root_ref`。
2. Cloud 校验 Project 和 ProjectBackendAccess。
3. Cloud 通过 Runtime Gateway 调用 local `workspace.detect`。
4. Local canonicalize 目标目录并执行 workspace_probe。
5. Cloud upsert BackendWorkspaceInventory；成功登记的 root 成为可用于 Workspace Binding 的事实。
6. 后续如果本机需要显式 workspace roots 投影，应从成功登记或本机 profile 中派生，而不是从启动参数派生。

### Execute

1. Session launch 从 Workspace Binding / VFS default mount 得到 `mount_root_ref`。
2. Relay prompt 将 `mount_root_ref` 发送到 local。
3. Local canonicalize `mount_root_ref`，并根据是否存在显式 workspace roots 决定是否做集合成员校验。
4. ToolExecutor 的每个相对 path / cwd / patch 解析都以 canonical workspace root 为边界。

## Migration

- 项目处于预研阶段，不引入旧行为兼容分支。
- `accessible_roots` 不再作为产品概念保留；涉及本机已确认 workspace root 集合的字段、DTO、TS 类型、Tauri profile、dev script、runtime health 持久化和前端展示统一朝 `workspace_roots` 迁移。
- 如果数据库仍持久化旧字段，需要通过 migration 调整到正确字段或正确语义。
- 实施时允许先完成行为修复再做命名收敛，但同一任务完成前不应留下用户可见的 `accessible_roots` 心智。

## Risks

- 放宽 `workspace.detect` 后，setup 流程可以读取 Git / P4 元数据；该能力来自用户在本机目录选择器中的主动选择，风险低于执行能力。
- 如果执行边界过度依赖 `mount_root_ref`，必须保证 `resolve_existing_path_with_root`、`resolve_path_for_write_with_root`、`resolve_shell_cwd`、apply_patch 路径解析测试充分。
- 多 root 与 session runtime 本机 SQLite 目录仍有历史耦合，需要避免在本任务中把 session db 继续绑定到“第一个 root”的错误模型。

## Spec Updates

- `cross-layer/project-backend-workspace-routing.md`：更新“register 不扩大 accessible_roots”的旧约束，改为 register 成功产生 workspace root 事实。
- `cross-layer/desktop-local-runtime.md`：记录本机默认全盘浏览和空 workspace roots 不构成异常。
- relay protocol 注释：移除 `workspace_detect` 必须位于 `accessible_roots` 内的陈述，改为本机校验目录存在且可访问；执行类工具仍受 `mount_root_ref` 边界约束。
