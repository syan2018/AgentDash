# Agent 工具体系剩余收口项

## 背景

这条任务创建时，项目的工具面仍然存在明显分叉。现在其中一部分已经完成：

- `tool_visibility` 已不再只用 `mcp_tools` 占位，而会展开到 MCP server 级别
- Project Agent / Story / Task 的条件工具注入已经比早期更清晰
- 统一 runtime 工具（`mounts_list` / `fs_*` / `shell_exec`）已经成为当前主路径

但仍有一些真正还在代码里的收口缺口：

- relay / local / API 仍保留 `command.workspace_files.*` 遗留协议链
- MCP tool 在 agent 侧仍以动态前缀名注入，和 hook/policy 对工具名的理解仍有错位
- 流程工具、runtime 工具、MCP tool 的最终 authority 和审批边界还没有彻底对齐

## Goal

把这条任务从“总论式 review”收缩为**剩余的真实收口项**，避免继续追踪已经完成的部分。

## 当前待解决的问题

### 1. 遗留 `workspace_files` 协议链仍未退场

当前仍然可见：

- relay 协议：`command.workspace_files.list` / `command.workspace_files.read`
- local handler 与 API 路由的配套实现

需要决定：

- 是否冻结为兼容层
- 是否迁移完现有调用后删除
- 文档中是否明确其仅为内部过渡链路

### 2. MCP tool 名称与 policy 观察视角仍不统一

当前 session plan 已能展示 MCP server，但：

- 注入到 agent runtime 时仍会生成带 server 前缀的动态名
- hook / policy / prompt 中仍常以工具名后缀来识别具体 MCP tool

需要收口：

- runtime 内部名与 policy 展示名的关系
- 审批/权限规则应绑定“完整名”还是“规范化名”
- 是否需要一层稳定的 tool identity 映射

### 3. 流程工具的 authority 仍需继续明确

当前 `report_workflow_artifact`、`companion_dispatch`、`companion_complete`、`resolve_hook_action`
已不再是纯粹“无差别注入”的状态，但仍需继续明确：

- 哪些属于基础运行时工具
- 哪些属于特定 session phase / flow capability 的条件工具
- 前端与 prompt 如何解释这些工具的可见性

## 非目标

- 不重命名当前全部 runtime tool
- 不在本任务内一次性重写 MCP 注入机制
- 不替代单个 MCP server 的产品设计

## Acceptance Criteria

- [ ] `workspace_files` 遗留链被明确标记为“保留兼容 / 冻结 / 删除”之一
- [ ] MCP tool 的 runtime 名称、policy 识别名和展示名关系被明确记录
- [ ] 条件流程工具的注入原则在代码与文档中保持一致
- [ ] 后续如继续拆任务，可以基于本任务拆成更具体的实现项
