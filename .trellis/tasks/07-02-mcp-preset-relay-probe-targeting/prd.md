# 收束 MCP Preset relay 探测目标

## Goal

让 MCP Preset 的连通性探测与运行时路由语义一致：声明为 `route_policy = relay` 的 Preset 必须在明确的本机 runtime 上执行 probe；没有可用本机 runtime 时给出产品化提示，而不是让云端直连本机地址或随机选择在线 backend。

## Background

当前用户在安装包中探测 `http://127.0.0.1:7321/expose/mcp` 时看到底层 `rmcp` 初始化错误。现场验证表明这台机器的 `ABCCopilot.Server` 确实在 `7321` 监听，问题不是端口不可达，而是当前 probe 链路在部分场景会忽略 MCP Preset 的 relay 语义。

已在当前工作区形成的基线补丁：

- `ProbeMcpPresetRequest` 带上 `route_policy`。
- `mcp.probe_transport` setup action 输入带上 `route_policy`。
- application probe 根据 `McpRoutePolicy::uses_relay(&transport)` 决定直连或 relay。
- 前端 probe cache key 纳入 `route_policy`，避免 direct / relay 共用同一结果。
- probe 错误展示支持长文本换行，避免撑破卡片和弹窗。

仍需规划和收束的问题是：relay probe 进入云端 `McpRelayProvider` 后，目前 `BackendRegistry::find_any_online_backend_for_setup_probe()` 直接从在线 backend map 中取任意一个 backend。这不表达“当前桌面客户端”或“用户选择的运行环境”，也不适合 localhost MCP。

## Requirements

- R1. Relay MCP Preset probe 必须有明确的 probe target，不得使用任意在线 backend 作为默认产品语义。
- R2. 在桌面安装包/客户端内触发的 relay probe，默认目标应由后端 helper 从当前用户自己的在线 Desktop local backend 中解析，不能信任前端随意指定一个 backend。
- R3. 如果当前客户端没有可用本机 runtime，probe 应返回结构化 unavailable / unsupported 结果，并向用户提示需要在客户端连接本机 runtime 后再探测。
- R4. 如果用户需要验证 Project runner 或其它非默认 backend，UI 必须显式选择目标运行环境，后端使用既有 `BackendAuthorizationService::require_backend(..., View)` 校验后路由 relay probe。
- R5. `route_policy = auto/direct` 的 HTTP/SSE Preset 保持现有直连语义；没有配置 relay 的本机地址失败时按普通连接失败展示。
- R6. 错误展示不应泄露过长底层类型名到布局层面；用户可见信息应优先表达 probe 位置、不可用原因和下一步动作。
- R7. 当前 `route_policy = relay` 的 HTTP/SSE probe 修复应保留回归测试，防止未来重新退回云端直连。

## Acceptance Criteria

- [ ] Relay probe API / Gateway input 能表达目标 backend 来源：当前用户默认本机 backend，或显式选择的 backend id。
- [ ] `route_policy = relay` 且 URL 为 localhost/loopback 的 Preset，在桌面客户端发起 probe 时由当前用户自己的在线 Desktop local runtime 执行。
- [ ] 没有当前用户在线 Desktop local runtime 时，前端展示“请在已连接的客户端中探测”类提示，不显示随机 backend 的连接错误。
- [ ] 同一用户存在多台在线 Desktop local backend 时，默认使用最近 claimed 的可用 backend，不增加运行环境选择交互。
- [ ] 用户显式选择 runner/backend 后，relay probe 路由到所选 backend；未授权或离线时返回稳定不可用提示。
- [ ] `route_policy = direct` 和 `auto + http/sse` 的 probe 行为不改变。
- [ ] 卡片、详情弹窗、Agent preset picker、Workflow capability 面板均不会被长错误文本撑破布局。
- [ ] 覆盖后端 relay target 解析、无 target 提示、显式 backend 路由、route_policy relay 分流、前端 typecheck 与 contract check。

## Out Of Scope

- 不重新设计运行时 Agent MCP list/call 的 `RuntimeBackendAnchor` 机制。
- 不改变 MCP transport、runtime_binding 或 shared library 安装模型。
- 不把 `auto_idle` 作为 MCP Preset probe 默认策略。
- 不为未声明 relay 的 localhost HTTP/SSE Preset 增加自动兜底。

## Decision

- D1. 不让前端默认携带不可信的 backend id。新增后端 resolver/helper，默认从当前用户拥有的在线 Desktop local backend 中解析 probe target；显式选择 runner/backend 时，后端按 backend 授权和在线状态校验。
- D2. 多个默认候选时选择最近 claimed 的在线 Desktop local backend，避免为 probe 增加额外运行环境选择交互。
