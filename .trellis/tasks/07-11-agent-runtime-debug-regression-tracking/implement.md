# Agent Runtime 调试回归实施计划

## ARD-001

- [x] 从 canonical Runtime Host definition inventory 与产品契约建立 execution profile projection。
- [x] 提供 discovery API，返回稳定 profile identity、availability 与 unavailable reason。
- [x] 以 LLM Provider catalog 与 profile 能力重建 discovered-options，不恢复 Connector。
- [x] 前端 selector 展示并禁用不可用项，暴露原因；删除对已删除路由契约的错误假设。
- [x] 在 ProjectAgent 写入边界复用同一 profile catalog 校验 agent type。
- [x] 补充 API、application 与前端回归测试。

## ARD-002

- [x] 删除 Web 层平台 token 前置门禁，将 token 改为可选字段。
- [x] 将 Tauri OAuth request、flow 与 HTTP helper 的 token 改为可选；仅在存在时附 Bearer。
- [x] 保留 API CurrentUser、系统管理权限与 OAuth flow owner 校验。
- [x] 补充 Personal 无 token、Enterprise 授权与前端/桌面 bridge 测试。

## Integration verification

- [x] 运行相关 Rust 与前端定向测试、format、clippy、typecheck、contracts check。
- [x] 使用 `pnpm dev` 验证执行器 discovery/options 与 Personal 无 token Provider OAuth prepare。
- [x] 独立 check Agent 复核架构边界与断链零残留。
- [x] 更新相关 Trellis spec；提交与推送由主会话完成。

## ARD-003

- [x] 将 create-run 合同拆为 model selection、runtime options 与 backend selection；executor 只来自 ProjectAgent。
- [x] 合并 ProjectAgent defaults 与 Provider/model override，保留 canonical executor 并写入 AgentFrame effective execution profile。
- [x] 将 backend selection 与 identity随 Runtime mailbox 传到 provision request。
- [x] Host offer selection 按 execution profile definition 和 explicit backend placement过滤。
- [x] Native/Codex source preparation按 execution profile选择 definition，Codex要求已有 activated offer。
- [x] 完成前后端、Runtime与真实 `pnpm dev` Draft 启动边界验证；提交推送由主会话完成。
