# 安装与本机运行形态产品化 - Implement

## Execution Map

总控顺序：

1. 启动 `runner-enrollment-token`，完成 runner registration token 与 claim API 契约。
2. 在 claim DTO 稳定后启动 `local-runner-daemon`，实现 headless runner 配置、凭据领取/写回、Linux/Windows 服务管理。
3. 与 1/2 并行启动 `windows-desktop-installer-background`，实现 Windows 桌面安装包、托盘、后台运行和自启动。
4. 当 runner 和 desktop 都输出稳定状态 snapshot 后启动 `runtime-diagnostics-settings`。
5. 当三类产物都可构建/安装后启动 `distribution-release-validation` 做 release gate。

推荐并行安排：

| 阶段 | 可并行任务 | 阻塞条件 | 输出 |
| --- | --- | --- | --- |
| Phase A | `runner-enrollment-token` + `windows-desktop-installer-background` | 无 | claim 契约；桌面生命周期契约 |
| Phase B | `local-runner-daemon` + desktop 安装器收口 | claim DTO 冻结 | runner service；Windows installer |
| Phase C | `runtime-diagnostics-settings` + release checklist 草案 | runner/desktop snapshot 冻结 | 统一状态 UI；验收矩阵 |
| Phase D | `distribution-release-validation` | 三类产物可构建 | release gate 通过 |

## Parent Coordination Checklist

- [x] 更新父任务状态板：每个子任务标记 `planning ready`、`in progress`、`blocked`、`ready for review`、`done`。
- [x] 每个子任务启动前检查上游依赖是否满足。
- [x] 每个子任务完成后把 handoff contract 回填到父任务，供下游任务消费。
- [x] 对并行任务维护文件写入边界，避免多个实现 agent 同时改同一批核心文件。
- [ ] 若子任务发现需要 runner 入站 HTTP API、跨平台桌面第一阶段、或混合版本发布，必须回到父任务 design 重新评审。

## Current Progress Snapshot

截至 2026-06-26：

| 子任务 | 状态 | 当前结论 | 后续处理 |
| --- | --- | --- | --- |
| `runner-enrollment-token` | done / archived | registration token、runner claim、ProjectBackendAccess、relay auth 边界已实现并归档 | 无 |
| `local-runner-daemon` | in progress | CLI/config/claim/status/log redaction、Linux systemd、Windows SCM service command 已实现 | 等 Linux/Windows 实机 service lifecycle、云端 online、断网重连验收后归档 |
| `windows-desktop-installer-background` | in progress | Desktop API `127.0.0.1:17301`、托盘、后台运行、显式退出、自启动、启动到托盘、自动连接 runtime 已实现 | 等 Windows NSIS 安装/卸载、登录自启动、托盘交互实机验收后归档 |
| `runtime-diagnostics-settings` | in progress | 进入实现阶段，消费 runner/desktop 状态与设置 handoff | 完成本地可验证 UI/类型/日志脱敏后可归档 |
| `distribution-release-validation` | planning / handoff target | 已补最终手工验收 checklist | 等三类产物和 diagnostics 全部通过实机验收后归档 |
| 父任务 `local-runtime-distribution` | planning | 负责最终集成收口 | 等所有子任务归档后归档 |

当前可并行空间：

- `runtime-diagnostics-settings` 可继续做本地实现和类型/测试验证。
- `distribution-release-validation` 可由接手者按 checklist 执行 Windows/Linux/云端实测。
- `local-runner-daemon` 与 `windows-desktop-installer-background` 的代码线不再需要大规模并行实现，主要等待真实环境验收 evidence。

## Subtask Execution Plans

### 1. Runner 注册令牌与云端领取流程

启动命令：

```powershell
python ./.trellis/scripts/task.py start 06-26-runner-enrollment-token
```

执行步骤：

- 读取 token 子任务 `prd.md`、`design.md`、`implement.md` 和 `implement.jsonl`。
- 先做 design review，冻结 token scope、claim endpoint、DTO、错误码、数据库字段、ProjectBackendAccess 规则。
- 实现 migration、domain entity/value object、repository。
- 实现 token create/list/revoke/rotate API。
- 实现 runner claim API，返回 runner 运行凭据。
- 生成/检查 contracts。
- 补测试覆盖 token 生命周期、claim 成功/失败、权限边界、过期/撤销。

完成条件：

- Claim API 契约可供 `local-runner-daemon` 使用。
- Handoff 写明 endpoint、请求/响应 DTO、错误响应、凭据字段、权限要求。

### 2. Local Runner 服务器守护进程交付

启动条件：

- `runner-enrollment-token` 的 claim endpoint 与 DTO 已冻结。

执行步骤：

- 固定 runner 配置文件格式、默认路径、CLI/env/file 优先级。
- 实现缺少运行凭据时 registration token claim，成功后原子写回运行凭据。
- 保留已有 WebSocket 出站主循环和断线重连。
- 增加日志文件输出、token 脱敏、status 输出。
- 实现 `service install|uninstall|start|stop|status`。
- 分别验证 Linux systemd 与 Windows Service。

完成条件：

- Linux/Windows runner 都可用 registration token 安装为服务并上线。
- Handoff 写明 service 名称、配置路径、日志路径、status 字段。

### 3. Windows 桌面安装包与后台运行

启动条件：

- 可以与 token 子任务并行启动。

执行步骤：

- 明确 `desktop:build` app exe 与 `desktop:bundle` setup exe 的产物边界。
- 实现系统托盘和菜单。
- 拦截关闭窗口，默认隐藏到托盘。
- 实现显式退出路径。
- 实现开机启动、启动到托盘、启动后自动连接 runtime 设置。
- 验证 Desktop API 默认绑定 `127.0.0.1:17301`，并确认普通 cloud/backend dev server 默认 `3001` 未被改动。
- 验证 NSIS 安装/卸载创建和清理系统项。

完成条件：

- Windows 桌面安装包可安装、后台运行、自启动、卸载。
- Handoff 写明桌面设置字段、托盘命令、Desktop API 状态字段、安装器产物。

### 4. 运行状态诊断与设置体验

启动条件：

- Runner 输出 status/log/restart 契约。
- Desktop 输出 runtime snapshot/settings 契约。

执行步骤：

- 定义统一状态 snapshot，区分 cloud API、Desktop API、Local Runtime/Runner、runner registration、relay connection。
- 增加前端类型、mapper、状态区域 UI。
- 接入日志 tail、清空日志、restart command。
- 接入自启动、启动到托盘、自动连接 runtime 设置。
- 编写错误文案，避免把 Desktop API 和 Local Runner 混称为本机 API。

完成条件：

- 用户能定位故障层级并执行恢复动作。
- Handoff 写明状态 DTO、UI 入口、日志脱敏测试。

### 5. 发布产物与验收流程

启动条件：

- Windows Desktop Installer、Linux Runner、Windows Runner 均有可构建产物。

执行步骤：

- 固化三类产物矩阵。
- 固化版本一致性检查。
- 编写 Windows Desktop、Linux Runner、Windows Runner 手工验收 checklist。
- 验证安装、启动、后台运行、自启动、service 生命周期、断网重连、卸载清理。

完成条件：

- Release gate checklist 可由非实现者执行。
- 三类产物全部通过验收。

## Review Dispatch Policy

- 每个子任务启动前派 `trellis-research` 或 `check` agent 做 planning review，输出设计缺口、实现拆分、风险和 handoff。
- 高风险子任务 `runner-enrollment-token`、`local-runner-daemon`、`windows-desktop-installer-background` 必须在实现前和实现后各 review 一次。
- `runtime-diagnostics-settings` 做 focused review，重点检查状态事实源、DTO、UI 文案和日志脱敏。
- `distribution-release-validation` 做 release checklist review，重点检查产物真实可安装、版本一致、卸载清理。

## Validation Commands

- Backend/API 子任务：`pnpm run contracts:check`、`pnpm run backend:check`、相关 `cargo test`。
- Runner 子任务：`cargo test -p agentdash-local`、Linux/Windows 服务命令 dry-run 或平台专项验证。
- Desktop 子任务：`pnpm run desktop:check`、`pnpm run desktop:bundle`，并进行 Windows 手工安装/卸载验收。
- Frontend/settings 子任务：`pnpm run frontend:check`、`pnpm run frontend:lint`、相关前端测试。
- 集成收口：按父任务 PRD 的 manual acceptance checklist 执行。

## Risk Points

- Runner registration token 的权限范围必须与 backend/project 可见性一致，否则会造成服务器 runner 过度授权。
- Windows Service 与桌面自启动是不同生命周期，不能复用同一个“开机启动”语义。
- Desktop API 默认绑定 `127.0.0.1:17301`；独立 runner 不应因为诊断需求引入业务 HTTP API。Desktop API 端口与普通 cloud/backend dev server 分开，原因是桌面安装包的内置 API 不应抢占常见本机 Web 调试端口。
- 日志、错误消息和配置导出必须脱敏 token 类字段。
- 父任务不能替代子任务的具体 planning；父任务只记录跨任务依赖和 handoff。
