# 实现计划：按 Identity 策略发现本机 Workspace

## 1. Contracts And Types

- 在 relay protocol 增加 `command.workspace_discover_by_identity` / `response.workspace_discover_by_identity` payload。
- 在 application runtime gateway 增加 `WORKSPACE_DISCOVER_BY_IDENTITY_ACTION`、input/output DTO 与 provider。
- 在 contracts crate 增加前端需要的 discover/bind request/response 类型并生成 TS。
- 在 backend inventory source 增加 `identity_discovery`，补 repository 映射与 migration。

## 2. Local Backend Discovery

- 新增 workspace identity discovery 模块，定义 strategy registry 与统一输出模型。
- 实现 unsupported identity skip。
- 实现 P4 strategy：
  - 解析 P4 identity contract。
  - 调用 `p4 clients -S <stream> --me` 获取候选 client。
  - 调用 `p4 client -o <client>` 读取 client spec。
  - 校验 root / alt root 可读。
  - 复用现有 `detect_workspace` 和领域 identity match 做二次确认。
- 将 handler 接入 local relay message dispatch。

## 3. API And Application Flow

- 扩展 BackendTransport，支持 `discover_workspace_by_identity`。
- 在 BackendRegistry adapter 中发送新 relay command 并映射返回。
- 新增 Project workspace route：
  - `discover-local-bindings`
  - `bind-discovered`
- `discover-local-bindings` 仅选择已授权且在线的 local backend。
- `bind-discovered` 重新 detect、match、upsert binding、upsert inventory。

## 4. Frontend

- 在 `backendAccess` 或 workspace service 中新增 discover/bind API client。
- 新增 `LocalWorkspaceDiscoveryPanel`，放入 Project 设置 `workspace` tab 的 Backend Access 与 Workspace List 之间。
- 面板支持：
  - 选择本机 backend。
  - 触发 discovery。
  - 展示 skipped 摘要。
  - 按 Workspace 展示候选。
  - 单个或多个候选绑定。
- 绑定成功后刷新 workspaces、candidates、inventory refresh key。
- 更新 Project 设置页文案，避免把主入口指向 Workspace 详情。

## 5. Validation

- 后端单元测试：
  - strategy registry skip unsupported identity。
  - P4 strategy 解析 client list / client spec。
  - 多候选返回给前端消歧。
  - stale root / mismatch 不生成候选。
  - binding upsert 幂等。
- 前端测试：
  - Project 设置页展示 discovery panel。
  - unsupported skipped 不进入候选列表。
  - 单候选与多候选绑定交互。
- 验证命令：
  - `cargo test -p agentdash-domain -p agentdash-application -p agentdash-api -p agentdash-local`
  - `pnpm test`
  - 如 contract 类型变化，运行现有 TS contract 生成/检查命令。

## Review Gate

开始实现前，确认本 task 已从 planning 切换到 in_progress，并加载相关 backend / cross-layer / frontend spec。
