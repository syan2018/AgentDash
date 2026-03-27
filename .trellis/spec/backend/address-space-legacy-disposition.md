# Address Space 遗留协议处置与路径语义规范

## 1. workspace_files 遗留协议处置

### 处置决定：冻结为内部兼容层（Frozen Internal Transport）

`workspace_files.list` / `workspace_files.read` relay 协议 **不再作为公共 API 或新功能接入点**。
它们被保留为 `relay_fs` provider 内部的 transport 实现。

**当前使用链路**:

```
Address Space Provider (public surface)
    ↓
relay_fs_provider (implementation)
    ↓
command.workspace_files.list / command.workspace_files.read (relay transport)
    ↓
local command_handler (本机执行)
```

**规则**:

1. **新功能禁止直接引用** `workspace_files` 协议——必须走 Address Space Provider
2. **现有引用** 仅保留在 relay transport 层（`protocol.rs`、`ws_handler.rs`、`command_handler.rs`）
3. **HTTP 路由** `/workspace-files/*` 保留为前端 @ 文件选择器的直连通道，后续应迁移到 `/api/address-spaces` 统一入口
4. **协议变体不再新增字段**，如有新需求走 `command.tool.*` 通道

### 受影响文件清单

| 文件 | 用途 | 处置 |
|------|------|------|
| `agentdash-relay/src/protocol.rs` | 协议定义 | 冻结，添加 deprecated 标注 |
| `agentdash-local/src/command_handler.rs` | 本机执行 | 保留，作为 relay transport handler |
| `agentdash-api/src/routes/workspace_files.rs` | HTTP 桥接 | 保留，标注为 legacy bridge |
| `agentdash-api/src/relay/ws_handler.rs` | 转发 | 保留 |
| `agentdash-api/src/relay/registry.rs` | 能力声明 | 保留 `supports_workspace_files: true` |
| `agentdash-api/src/address_space_access/mod.rs` | provider 声明 | 正常使用 |

---

## 2. 路径语义统一规则

### 2.1 资源定位模型

所有上层 API 统一使用 `mount_id + relative_path` 定位资源。

```
mount_id: "main"    → Task 绑定的执行 workspace
mount_id: "spec"    → Project 级规范容器
mount_id: "brief"   → Story 级 brief 容器
```

### 2.2 路径规则

| 规则 | 说明 |
|------|------|
| 绝对路径 | **禁止** — provider 必须拒绝绝对路径输入 |
| `..` 越界 | **禁止** — 路径规范化后不得逃逸 mount 根目录 |
| 路径分隔符 | 统一使用 `/`，provider 负责 OS 适配 |
| 前导 `/` | 被 strip，视为相对路径 |
| 空路径 | 代表 mount 根目录 |

### 2.3 cwd 解析规则

`shell.exec` 工具的 `cwd` 参数遵循以下规则:

| 输入 | cloud (relay) | local (direct) |
|------|---------------|----------------|
| 相对路径 (如 `src/`) | 相对 workspace root 解析 | 相对 workspace root 解析 |
| 绝对路径 | **拒绝** | **拒绝**（Hook Runtime 已有 rewrite 兜底） |
| 空/省略 | 默认为 workspace root | 默认为 workspace root |
| `.` | workspace root | workspace root |

**注意**: Hook Runtime 在 `agentdash-connector-contract/src/hooks.rs` 中对位于 workspace root 内的绝对 cwd 做了自动 rewrite（参见 AGENTS.md 问题说明），但这是兜底行为，不应被依赖。

---

## 3. MCP Tool 命名关系

MCP (Model Context Protocol) 工具在系统中有三种名称形态：

| 名称类型 | 用途 | 示例 |
|----------|------|------|
| **runtime 名称** | Agent 实际调用时使用的工具名 | `fs_read`, `fs_write`, `shell_exec` |
| **policy 识别名** | Hook policy 中匹配工具的 key | `fs.read`, `fs.write`, `shell.exec` |
| **展示名** | 前端 UI / 日志中显示的名称 | `文件读取`, `文件写入`, `命令执行` |

映射关系：
- runtime 名称使用 `_` 分隔（适配大多数 LLM tool calling 规范）
- policy 识别名使用 `.` 分隔（与 mount + path 模型的层级语义一致）
- 展示名由 `tool_visibility.tool_names` 提供，或由前端本地化
- 转换规则：`runtime_name.replace('_', '.')` ↔ `policy_name.replace('.', '_')`

---

## 4. 流程工具注入原则

`report_workflow_artifact` 等工作流感知工具的注入遵循以下原则：

1. **条件注入**: 仅在 Hook Runtime 检测到活跃工作流时注入，非工作流 session 不可见
2. **authority 一致性**: 工作流工具的权限受 mount capability 约束——如果 mount 无 `write` 能力，工作流工具也不应修改该 mount 下的资源
3. **命名约束**: 工作流工具以 `workflow.` 前缀命名，避免与 `fs.*` / `shell.*` 基础工具冲突
4. **生命周期**: 工作流工具的注入和撤回与工作流 step 生命周期绑定
