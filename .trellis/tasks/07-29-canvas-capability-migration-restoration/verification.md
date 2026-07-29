# Canvas 能力回迁验收

## 架构边界

- Workspace Module core 只包含 provider registry、opaque module identity、descriptor projection、
  operate/invoke/present 通用路由与五个 Agent 工具。
- Canvas definition/runtime projection、create/attach/copy、Interaction attachment、authoring mount、
  ResourceSlot、VFS 与 runtime Operations 全部由系统内嵌 Canvas provider 持有。
- Canvas provider 仅在 API composition root 注册；Workspace Module API 与 core 不识别 Canvas、
  Interaction、mount 或 AgentFrame。
- AgentFrame 只作为 Agent 调用 create/copy/attach 时 authoring mount 收敛的内部持久化机制；
  present、binding、state、diagnostics 与 source mutation 不写 AgentFrame。

## 能力验收

- `workspace_module_list/describe/operate/invoke/present` 已恢复；operate 保留
  `canvas.create/attach/copy`，不存在 `canvas.request`。
- Canvas asset lifecycle、SourceBundle files editor、binding editor、standalone/attached preview、
  MessageChannel SDK、exact Operation invoke、asset URL、canonical Interaction command/event、
  renderer observation 与 Agent input submit 已接入新底层。
- Canvas VFS 支持 read/list/search/write/delete/rename，无 exec；单 mount 的多文件
  `fs_apply_patch` 使用 provider 原生整包 changeset，只生成一个 immutable definition revision。
- `canvas-system` 与 `workspace-module-system` embedded Skill 已同步当前合同并通过校验。
- 旧 application 内重复 Workspace Module projection 与旧开放式 Canvas bridge 已删除。

## 验证结果

- `cargo check -p agentdash-api -p agentdash-workspace-module -p agentdash-application-vfs`
- `cargo test -p agentdash-workspace-module`：6 passed
- `cargo test -p agentdash-application-vfs`：169 passed
- `cargo test -p agentdash-domain canvas_skill`：1 passed
- `cargo clippy -p agentdash-workspace-module --lib --no-deps -- -D warnings`
- `pnpm run contracts:check`
- 任务范围 ESLint
- `pnpm --filter app-web run typecheck`
- Canvas focused Vitest：16 passed
- 两个 embedded Skill `quick_validate.py`
- `git diff --check`

全仓 ESLint 与全目标 Clippy 仍存在本任务外的既有告警；本任务文件的 ESLint、Workspace Module
Clippy 与编译测试均已通过，未修改告警所属的其它文件。
