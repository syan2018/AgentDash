# Agent 侧 Canvas 接口

这些是 Agent 工具，不是 iframe SDK。

## 生命周期

- `workspace_module_operate(operation="canvas.create", input={ title, description?, canvas_mount_id? })`
- `workspace_module_operate(operation="canvas.attach", input={ canvas_mount_id })`
- `workspace_module_operate(operation="canvas.copy", input={ source_mount_id, canvas_mount_id?, title?, description? })`
- `workspace_module_list`
- `workspace_module_describe(module_id="canvas:{definition_id}")`

create/copy 创建 personal definition 并走同一 create mount 链；attach 不复制 definition。
三者返回 `canvas_mount_id`，随后用 `{canvas_mount_id}://...` 进行 VFS authoring。

## VFS

- `fs_read`：读取 source/binding 与 version。
- `fs_glob` / `fs_grep`：列出、搜索 SourceBundle。
- `fs_apply_patch`：在 writable personal mount 上原子生成新 revision。
- Canvas mount 不支持 exec；project shared 和 `bindings/*` 保持只读。

## Runtime

present 后重新 list/describe `interaction:{instance_id}`，再按 descriptor 调用：

- `canvas.bind_data`：绑定 declared ResourceSlot，不修改 source/state。
- `canvas.inspect`：读取 latest renderer observation。
- `canvas.get_interaction_state`：读取 Agent allowlisted canonical state。
- `workspace_module_present`：创建或复用 presentation attachment 并返回 canonical URI；不挂载
  source、不改变 AgentFrame。

只按 descriptor 的 operation key、schema、readiness 与 exact OperationRef 行动。
