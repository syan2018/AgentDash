# ProjectAgent backend requirement 实施计划

## Checklist

- [x] 在 domain `AgentPresetConfig` 增加 `backend_requirement` 枚举字段，纳入 merge / normalize / serde。
- [x] 更新 contracts 生成与前端 ProjectAgent 类型。
- [x] 更新 ProjectAgent preset form state 与字段 UI，默认值为 `required`。
- [x] 在 ProjectAgent start / mailbox launch planning 中传递 backend requirement。
- [x] 调整 backend placement 解析：`optional` 自动解析失败时允许无 placement，显式选择失败仍报错。
- [x] 调整 frame construction / owner bootstrap：无真实 backend placement 时不构造 backend-bound `main` workspace。
- [x] 补后端单测覆盖 required 默认、optional 无 backend、optional 有 backend、optional 显式离线 backend。
- [x] 补前端单测覆盖表单默认值与保存值。

## Validation Commands

- `cargo test -p agentdash-domain agent_config`
- `cargo test -p agentdash-application-agentrun`
- `cargo test -p agentdash-application-runtime-session backend_execution_placement`
- `pnpm --filter app-web test -- project-agent`
- `pnpm frontend:check`

## Review Notes

- 重点检查 runtime surface 是否真实反映 backend placement。
- 重点检查存量 ProjectAgent config 缺省行为是否保持 `required`。
