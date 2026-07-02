# ProjectAgent backend requirement 设计

## Contract

在 `AgentPresetConfig` 增加 `backend_requirement?: "required" | "optional"`。该字段属于 ProjectAgent preset config，原因是它描述 Agent 自身运行环境要求，而不是某一次用户输入的 backend 选择。

缺省值为 `required`，后端解析、前端表单初始化和 summary 展示都按同一默认值处理。

## Data Flow

1. ProjectAgent create/update 保存 `config.backend_requirement`。
2. ProjectAgent start / AgentRun mailbox 从 ProjectAgent config 解析 backend requirement。
3. launch planning 根据 backend requirement 处理 backend selection：
   - `required`：无可用 backend 是错误。
   - `optional`：自动解析不到 backend 时返回无 backend placement。
   - 显式 backend selection 始终要求目标 backend 在线可用。
4. frame construction 只在存在真实 backend placement / runtime backend anchor 时构造 backend-bound `main` workspace surface。
5. 无 backend placement 时 capability / VFS / MCP projection 不暴露依赖本机 backend 的能力。

## Frontend

ProjectAgent preset editor 在运行环境区域增加一个二选一控件：

- `必须有可用机器`
- `机器可选`

表单状态、ProjectAgent 类型和 form serialization 均包含 `backend_requirement`。空 config 初始化为 `required`。

## Compatibility

无需数据库 migration。ProjectAgent config 是 JSON blob，缺省按 `required` 解释即可保持存量行为。
