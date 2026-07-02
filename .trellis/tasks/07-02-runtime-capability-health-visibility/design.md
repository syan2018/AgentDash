# 平台声明能力健康状态设计

## Problem

平台已向用户声明的能力（MCP tools、executor 等），在启动/发现/连接/调用时可能不可用。当前失败只写 `diag!` 日志，用户看到的是工具缺失或调用报错，没有结构化的"这些能力挂了"提示。

## Boundary

能力健康项只从平台声明面派生。声明面 = runtime 注册 payload、local runtime status、backend runtime summary、tool/capability catalog、session runtime surface 中已经承诺给用户或执行路径的能力。

未配置、未启用、未安装的外部对象不进入 capability health。

## Health Model

### 状态

用户可见状态 3 档：

| Status | 含义 |
|--------|------|
| `ready` | 能力可用 |
| `degraded` | 部分可用，存在非阻断故障 |
| `unavailable` | 不可用 |

内部过渡态 `connecting` / `probing` 不作为独立用户可见状态暴露到前端 contract；前端在收到 probe/connect 开始事件时可选择展示 loading 指示器，但 DTO 不定义额外状态枚举。

尚未被探测或使用的能力不主动展示为任何状态——惰性模型下，只有触发过交互的能力才进入 health surface。

### Contract DTO — `CapabilityHealthItem`

前端消费的字段精简为：

```
id:       稳定标识，如 mcp:<server-name>、executor:<executor-id>
domain:   能力域枚举，MVP 为 mcp | executor
status:   ready | degraded | unavailable
label:    用户可读名称
summary:  一句话摘要（错误原因或影响）
actions:  可操作入口列表，如 probe / retry / open_settings / view_logs
```

内部诊断元数据（owner、declaration_source、severity、详细 error chain）保留在 Rust 层 `diag!` 和 diagnostics endpoint 中，不进跨层 contract DTO。这样 contract 表面积小，前端渲染逻辑简单，排障时通过 diagnostics 入口下钻。

`domain` 枚举可扩展（`#[non_exhaustive]` / union type），未来域接入只加枚举值和对应 health producer，不改 contract 结构。

## MVP Domains

### MCP

状态规则：

- `probe_mcp_server` 或 `list_tools` / `call_tool` 成功 → `ready`
- stdio spawn、握手、HTTP/SSE 连接、tools/list、tool call 失败 → `unavailable` 或 `degraded`（部分 tool 可用时为 degraded）
- 保持惰性：启动期不主动连接所有 server。用户 probe、工具发现或真实调用时回写状态。

### Runtime/Runner Executor

- runtime register payload 中出现的 executor，在 backend 在线且 `available=true` / `allocatable=true` 时为 `ready`。
- `available=false` 或不可分配时为 `degraded`。
- runtime 离线时为 `unavailable`。
- 只有已由 runtime 声明过的 executor 才创建 health 项。

## Data Flow

1. Local runtime 加载 MCP config，构建 `McpClientManager`（不连接）。
2. probe/list/call 路径成功或失败时，local runtime 更新 MCP health 快照。
3. Relay register / `EventCapabilitiesChanged` 将 capability health 快照随 `CapabilitiesPayload` 上报 backend。
4. Backend runtime summary 从 capabilities payload 派生 health 投影，暴露给前端。
5. Executor health 在 runtime summary 层从 executor 状态派生。
6. Frontend diagnostics 消费 local runtime status 与 backend runtime summary，展示声明能力列表。
7. Session 侧筛选本次 runtime surface 中 `degraded` / `unavailable` 项，展示能力不完整提示。

## Persistence

使用现有 `runtime_health.capabilities` JSON 字段作为在线快照载体，不新增数据库表。

## UI

- Local Runtime / Settings diagnostics 增加声明能力区域，MCP 按 server 展示状态。
- 失败项给出 error summary + probe/retry/settings/logs 入口。
- Backend/runtime 选择入口展示 executor health，不可用时提示影响。
- Session 侧在本次 runtime surface 中有 `degraded/unavailable` 时展示 inline notice。

## Risks

- 惰性模型下，未触发交互的 MCP server 不会出现在 health surface。用户需要主动 probe 或等待首次调用才能发现问题。这是可接受的取舍——主动 probe 全部 server 会拖慢启动。
- capability health 与 backend 级 `runtime_health.status` 共存。后者是整体连接状态，前者是细粒度能力可用性，互不替代。
