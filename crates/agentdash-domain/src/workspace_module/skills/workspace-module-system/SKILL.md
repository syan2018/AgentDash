---
name: workspace-module-system
description: 发现、检查、调用、组合、展示并诊断当前 actor 可见的 Workspace Module。用于选择 workspace_module_operate/list/describe/invoke/present 或 operation_script，处理 Canvas、Interaction、Extension、原生 VFS/Process/Task Operations，以及解释空或 degraded capability surface。
---

# 使用 Workspace Module

把 Workspace Module descriptor、Operation、view、readiness 与 state projection 当作服务端生成的
当前快照。只使用最新 `list/describe` 返回的 identity 与合同，让 OperationGateway 负责当前权限、
placement、schema、执行和审计。

## 选择工具

- 用 `workspace_module_operate` 执行 provider-owned lifecycle route，例如
  `canvas.create`、`canvas.attach`、`canvas.copy`。
- 用 `workspace_module_list` 发现当前 actor 可见的 modules，并读取 surface readiness。
- 用 `workspace_module_describe` 取得一个 module 的当前 views、operation keys 与 exact OperationRefs。
- 用 `workspace_module_invoke(module_id, operation_key, input)` 调用最新 descriptor 中的一项 Operation。
- 用独立的 `operation_script` 对当前 actor surface 做有界、即时、跨 module 的多 Operation 组合。
- 用 `workspace_module_present` 打开最新 descriptor 中声明的 UI view。
- 需要 durable retry、recovery、human gate 或跨 session 状态时使用 Workflow，不使用 OperationScript。

## 核心流程

1. 调用 `workspace_module_list({})`。
2. 先检查 `surface_readiness`；若为 `degraded`，同时检查 `surface_diagnostics`。
3. 选择返回的准确 `module_id`，调用 `workspace_module_describe(module_id)`。
4. 只使用最新 describe 返回的 `operation_key`、`operation_ref` 和 `view_key`。
5. stale ref、readiness、capability、attachment 或 revision 失败后重新 list/describe，不缓存、拼接
   或猜测 identity。

单次调用只把 `module_id + operation_key + input` 交给 `workspace_module_invoke`。只有
`operation_script` 源码内部才使用 describe 返回的 exact OperationRef 字符串。

## Module identity

- `canvas:{definition_id}`：Canvas authoring definition。
- `interaction:{instance_id}`：当前 attachment 可见的 shared Interaction runtime。
- `ext:{extension_key}`：已安装 Extension。
- `builtin:vfs`、`builtin:process`、`builtin:task`：无 UI 的原生 platform Operations。
- 其它 provider 的 module ID 与 kind 视为 opaque。

Canvas 的 source 编辑、runtime bridge、binding、diagnostics 与视觉交付细节使用
`canvas-system` Skill。创建、挂接或复制 Canvas 时分别调用：

```text
workspace_module_operate(operation="canvas.create", input={title, description?, canvas_mount_id?})
workspace_module_operate(operation="canvas.attach", input={canvas_mount_id})
workspace_module_operate(operation="canvas.copy", input={source_mount_id, title?, description?, canvas_mount_id?})
```

## 按需读取

- 需要组合多个 Operations 时，读取
  [OperationScript 组合](references/operation-scripts.md)。
- surface 为空、degraded 或 Operation 不可用时，读取
  [Surface 诊断](references/surface-diagnostics.md)。
