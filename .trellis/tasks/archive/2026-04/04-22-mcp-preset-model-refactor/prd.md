# MCP Preset 模型与 Agent Preset 引用重构

## 背景

当前 MCP Preset / Agent MCP 配置存在以下混淆：

- `preset.name` 同时承担展示名与引用 key 语义
- `server_decl.name` 承担 agent 侧 server identity，和 `preset.name` 高度重复
- `relay` 放在 `server_decl` 内，但它本质上是应用层路由策略，不属于 transport 连接定义
- Agent 配置仍允许直接编辑原始 `mcp_servers`，导致 project 级 preset 与 agent 级 inline server 成为两套定义源

项目处于预研阶段，不需要兼容旧设计，目标是直接收敛到更清晰的正确模型。

## 目标

### 1. Preset 模型分层清晰

MCP Preset 应拆分为：

- `key`: 项目内唯一，既是 preset 引用 key，也是 agent-facing server name
- `display_name`: 纯展示字段
- `description`
- `transport`: 仅包含 transport 相关字段（http/sse/stdio）
- `route_policy`: 应用层路由策略（`auto | relay | direct`）

### 2. 引用语义统一

- Workflow / capability / session runtime 中的自定义 MCP 一律解释为 `mcp:<preset_key>`
- 不再存在“按 preset 名引用”和“按 server 名注入”两套并行命名语义

### 3. Agent 配置只允许引用 preset

- 移除 Agent 配置中的原始 `mcp_servers`
- Agent 配置改为仅存储 preset 引用列表
- Agent 页面可以提供“快速创建 MCP Preset”的入口，但定义最终仍落到 project 级 preset 资产

### 4. UI 心智统一

Preset 表单只暴露：

- 显示名称
- 工具标识（默认由显示名称生成，可手动修改）
- 连接方式 / transport 配置
- 路由策略
- 描述

不再暴露两套高度相似的 name 字段。

## 非目标

- 不做向后兼容层
- 不保留旧 `mcp_servers` Agent 配置写法
- 不做分阶段双写 / 双读

## 实施范围

### 后端

- `agentdash-domain::mcp_preset`
- `agentdash-application::mcp_preset`
- `agentdash-application::capability`
- `agentdash-application::session`
- `agentdash-api` 的 mcp preset / project agent 路由与 DTO
- `agentdash-infrastructure` 的 mcp preset 持久化与迁移
- `agentdash-spi` 的 MCP capability 注释与命名

### 前端

- MCP Preset 类型与 service mapper
- Assets 页 MCP Preset 面板
- Agent 页 MCP 配置 UI
- 相关 workflow / capability 选择器文案与字段消费

## 验收标准

- MCP Preset DTO/实体中不再出现 `server_decl.name` 与 `server_decl.relay`
- `mcp:<...>` 一律按 preset `key` 解析
- Agent 配置中不再保存原始 `mcp_servers`
- Agent 页面仅允许选择 preset，并支持快速创建 preset
- Preset UI 中明确区分 `display_name` 与 `key`
- route policy 与 transport 字段边界清晰
