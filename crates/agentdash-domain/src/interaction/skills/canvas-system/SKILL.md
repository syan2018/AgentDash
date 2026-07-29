---
name: canvas-system
description: 创建、编辑、检查、展示和完善由 InteractionDefinition 与 InteractionInstance 承载的 AgentDashboard Canvas。用户要求 Canvas、交互式可视工作区、Canvas 源码修改或预览、共享交互状态与命令、嵌入 Extension 组件，或通过 Workspace panel 交付可用视觉结果时使用。
---

# 构建 Canvas 体验

把 Canvas 视为系统内嵌的通用 Workspace Module provider。始终区分 definition source、
authoring mount、runtime state、attachment authority 与 UI presentation。

## 使用规范流程

1. 调用 `workspace_module_list`，再 describe 当前 actor surface 返回的准确
   `canvas:{definition_id}` 或 `interaction:{instance_id}`。
2. 使用 `workspace_module_operate` 创建、挂接或复制 Canvas；修改 source 前读取
   [authoring.md](references/authoring.md)。
3. 只调用最新 descriptor 返回的准确 Operation。不要重建 OperationRef，也不要用内部 HTTP route
   替代 Agent authority。
4. 通过 authoring mount 和通用 VFS 工具编辑文件；mount 不支持 exec。
5. 使用 `workspace_module_present` 展示已有 definition。服务端创建或复用 presentation attachment
   后返回 canonical `interaction://{instance_id}` runtime identity；present 不挂载 source。
6. iframe 需要 Operation、资产、Interaction、Agent 回流或诊断时先读取
   [runtime-bridge.md](references/runtime-bridge.md)。
7. 交付前读取 [presentation-quality.md](references/presentation-quality.md)。
8. revision、attachment、permission、readiness 或 capability 变化后重新 describe。

## 保持身份边界

- `canvas:{definition_id}` 标识 authoring definition module。
- `canvas://{definition_id}` 标识 definition preview。
- `{canvas_mount_id}://...` 标识 authoring VFS 文件。
- `interaction:{instance_id}` 标识已 attach 的 shared runtime module。
- `interaction://{instance_id}` 标识 runtime presentation。
- definition revision 固定 immutable `SourceBundle`；source 修改创建新 revision。
- attachment 只授予 actor 访问 instance 的能力，不是 definition 或 instance 本身。
- presentation URI 只负责打开 UI，不是执行或写入 authority。

## 遵守当前 capability surface

Workspace Module core 只负责 provider 发现和路由，不拥有 Canvas source authority。Canvas provider
返回的 create/attach/copy 结果包含准确 authoring mount；不要猜测 mount、调用过时 Canvas API
或把 browser-only state 当作业务事实。

把 projected Interaction state 当作只读证据。状态修改必须通过已 describe 的 command Operation
提交，并携带当前 expected state revision。

## 交付真实可用的 Canvas

构建用户要求的可操作界面，而不是装饰性 landing page。保持可见文案简洁，保存有意义的交互状态，
暴露清晰命令，并展示完成后的 Canvas 供用户检查。

## 按需参考

- Agent 侧创建、挂接、复制、VFS、bind、inspect、state 与 present：
  [agent-side-interfaces.md](references/agent-side-interfaces.md)
- iframe SDK 总入口：[runtime-bridge.md](references/runtime-bridge.md)
- exact Operation 调用：[runtime-actions.md](references/runtime-actions.md)
- VFS 图片资源：[vfs-assets.md](references/vfs-assets.md)
- canonical Interaction state：[interaction-state.md](references/interaction-state.md)
- 用户显式回流 Agent：[agent-submit.md](references/agent-submit.md)
