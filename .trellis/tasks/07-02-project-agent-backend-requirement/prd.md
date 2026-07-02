# ProjectAgent backend requirement 配置

## Goal

ProjectAgent 支持配置“backend 是否必须存在”。默认和存量 Agent 均要求 backend；配置为 backend 可选的 Agent 在没有可用 backend 时仍可启动，但运行面必须真实表达“无本机机器能力”，不能虚构 `main` workspace 或 backend-bound capability。

用户价值：

- 需要本机 workspace / MCP / terminal / extension host 的 Agent 继续获得明确的不可用提示。
- 纯云端能力的 Agent 可以在没有在线机器时使用，不被 workspace backend 状态阻断。

## Requirements

- 新增 ProjectAgent preset 配置字段 `backend_requirement`，取值为 `required | optional`。
- `backend_requirement` 缺省时按 `required` 处理；存量 ProjectAgent 不需要迁移即可保持当前行为。
- 前端 ProjectAgent 编辑表单提供运行环境选项：
  - `必须有可用机器` -> `required`
  - `机器可选` -> `optional`
- `required` 模式延续当前强约束：启动或投递时没有在线可用 backend / executor 时返回可见错误。
- `optional` 模式下：
  - 显式选择 backend 时仍校验该 backend 在线可用。
  - 有真实在线 backend 可用时允许使用 backend-bound workspace 与本机能力。
  - 没有可用 backend 时允许启动，但 launch planning / frame construction 不得生成虚构 backend placement。
  - 无 backend placement 的 runtime surface 不包含 backend-bound `main` workspace、本机 MCP、terminal、extension host 等能力。

## Acceptance Criteria

- [ ] 新建 ProjectAgent 默认展示为 `必须有可用机器`。
- [ ] 存量 config 缺少 `backend_requirement` 时，后端和前端均按 `required` 解释。
- [ ] ProjectAgent 可保存并读取 `backend_requirement = optional`。
- [ ] `required` Agent 在无可用 backend 时返回用户可见的 unavailable 错误。
- [ ] `optional` Agent 在无可用 backend 时可以创建 AgentRun / RuntimeSession，并且 runtime surface 不包含虚构 backend-bound `main` workspace。
- [ ] `optional` Agent 在存在在线 backend 时仍可使用真实 backend placement。
- [ ] `optional` Agent 显式选择离线 backend 时仍返回用户可见错误。
- [ ] 前端单测覆盖表单默认值、保存值和存量缺省展示。
- [ ] 后端单测覆盖配置解析、launch planning、frame surface 的无 backend 场景。

## Notes

- 需求已收敛为 `required | optional` 两态，因为当前产品决策只需要表达 Agent 对 backend 的硬性依赖程度。
- 语义重点是“backend 可选”：Agent 可以在没有 backend placement 时运行，但任何 backend-bound surface 都必须来自真实在线 backend。
