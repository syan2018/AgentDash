# MCP 概念模型收束重构

## Goal

收束项目内 MCP 相关概念、数据结构和跨层转换，保留真实业务语义，删除无法证明独立价值的模型、投影和兼容入口。

本项目仍处预研阶段，本任务按硬收束处理：不保留 API / 数据结构兼容层，不为旧字段、旧类型或旧调用路径设置回退逻辑；涉及数据库字段变化时通过 migration 让 schema 进入目标状态。

## User Value

- 开发者能清楚判断 MCP 在当前系统中只有哪些事实源、边界 DTO 和展示投影。
- Session / Capability / Relay / Local Runtime / Frontend 不再各自维护近似同构的 MCP transport 或 server 定义。
- 后续新增 MCP 能力时只能接入目标模型，不能继续扩大概念数量。

## Confirmed Facts

- `agentdash_domain::mcp_preset::McpPreset` 是项目级 MCP 资产，包含 `key`、`display_name`、`transport`、`route_policy`、`runtime_binding`、`source` 等字段。
- `agentdash_domain::mcp_preset::McpTransportConfig` 已是 domain transport shape，`agentdash_spi` 直接 re-export。
- `agentdash_spi::RuntimeMcpServer` 是执行面 MCP server，包含 `name`、`transport`、`uses_relay`。
- `agent_frames.mcp_surface_json` 持久化的是 runtime MCP surface 的 JSON 投影，不成为独立业务模型。
- Relay wire 使用 `McpTransportConfigRelay` / `McpServerRelay`，转换集中到 application adapter。
- `agentdash_application::runtime::McpServerSummary` 是只读展示投影，执行面不从 summary 反向生成。
- 前端 generated contract 与 `packages/core/local-runtime` 各自保留所属边界类型，`McpTransportConfigEditor` 使用 views 内部编辑值类型。
- Custom `mcp:<key>` 能力只从 project MCP Preset 解析为 `RuntimeMcpServer`。
- Marketplace `mcp_server_template` 是安装模板，不是执行态 server。

## Requirements

1. 概念数量收束到少数可解释对象：
   - `McpTransportSpec` / 当前 `McpTransportConfig`：纯连接参数。
   - `ProjectMcpPreset` / 当前 `McpPreset`：项目资产事实源。
   - `RuntimeMcpServer`：执行面事实源。
   - `McpServerRelay`：Relay/API 边界 DTO。
   - `McpServerSummary`：只读展示投影。
   - `McpServerTemplate`：Marketplace 安装模板。
2. 所有 MCP transport 与 server 转换集中到单一 adapter 边界，route、handler、manager 中不再重复 match transport enum。
3. 执行面仅以 runtime server 作为事实源；summary/bootstrap/view 类型不得反向生成执行面配置。
4. Agent 配置只表达 MCP preset 引用；custom MCP 能力不走 inline server fallback。
5. 前端 MCP Preset UI 使用 generated contract 类型；本机 runtime UI 使用 local runtime 配置类型；共享 editor 只能消费显式抽象或由调用侧 adapter 包装。
6. Marketplace MCP template 只负责安装为 project preset，不参与 session runtime 命名、capability key 或 relay dispatch。
7. 数据库字段与 migration 跟随目标模型清理；不保留废弃字段或兼容读取。
8. 文档和 spec 只记录目标模型为什么这样分层，不记录过渡实现。
9. 实施顺序必须先锁定目标数据结构和模块归属，再允许各模块做删除式重构；未锁定前不得让模块各自发明临时适配。
10. 后续模块重构以 subagent 分派为默认执行方式，每个 subagent 负责一个可独立验证的模块边界，并以删除旧结构、旧转换、旧 fallback 为主要产出。

## Acceptance Criteria

- [x] 仓库内 MCP 相关数据结构有清晰分层清单，超过目标清单的结构已删除、改名为投影，或被限制为边界私有 DTO。
- [x] `McpTransportConfig` 与 relay/local/API DTO 的互转只存在一个权威 adapter 实现。
- [x] `RuntimeMcpServer` 成为唯一可执行 MCP surface 类型。
- [x] 有损 summary 反向生成 `RuntimeMcpServer` 的路径不存在，summary-only 类型不会进入执行面。
- [x] Inline MCP server fallback 被删除，测试围绕 preset key 主路径。
- [x] 前端共享 MCP transport editor 不再隐式依赖 local-runtime 类型来编辑 cloud MCP Preset。
- [x] Relay prompt、relay MCP probe/list/call、local MCP config、capability resolver、AgentFrame projection 仍能围绕同一个 runtime MCP surface 工作。
- [x] 数据库 migration 与 repository 映射通过验证，schema 不保留无价值 MCP 字段。
- [x] 相关 lint、类型检查和定向测试通过。
- [x] 第一阶段完成后，目标 MCP 类型、模块归属、允许存在的边界 DTO、禁止反向转换的规则已经在代码中锁定，并被后续模块重构复用。
- [x] 每个模块重构交付都能证明没有新增兼容层、重复类型或重复转换。

## Out Of Scope

- 不新增 MCP 协议能力。
- 不重新设计权限 grant 模型。
- 不扩展 marketplace 资产类型。
- 不调整 MCP 连接实现本身，除非类型收束要求移动 adapter。

## Closure Notes

- Marketplace `mcp_server_template` 保留为安装模板，边界固定为 `McpServerTemplate -> ProjectMcpPreset`，不进入执行态。
- 本次没有 MCP 数据库字段删除或 repository schema 变更；现有 `agent_frames.mcp_surface_json` 持久化 runtime surface，目标模型不需要新增 migration。
- scoped search 覆盖代码、packages、docs、spec、当前任务和相关活跃任务；旧执行类型名与 inline server 路径无残留。
