# Agent Runtime 重构调试回归长期追踪

## Goal

在 Agent Runtime 架构收敛后的持续调试期内，集中登记、复现、诊断和修复影响真实产品路径的问题，使 Integration、Managed Runtime、AgentRun、LLM Provider、Relay、本机运行时与前端控制面在实际启动和交互中形成可验证的完整闭环。

本任务作为长期父账本保留。每个问题都必须能够从用户现象追踪到所属事实源、composition root、协议边界或 UI projection，并以真实产品路径验证修复结果。

## Requirements

### R1. 问题登记与生命周期

- 每个调试问题使用稳定编号登记，至少记录现象、复现入口、期望行为、影响范围、当前状态和证据。
- 问题状态使用 `reported → reproduced → diagnosed → fixed → verified`；无法复现时保留已尝试条件和仍缺失的证据。
- 同一根因导致的多个表象应收敛到一个问题项；不同事实源或独立验收边界的问题保持独立。
- 修复应落在正确的架构边界，不通过兼容路径、双事实源、静默 fallback 或前端硬编码恢复表象。
- 新发现的问题直接追加到本父任务；仅当某一问题形成可独立规划、实施和验收的大型交付时，才在本目录下建立 workstream。

### R2. Integration 执行器发现与选择

**问题 ARD-001：平台前端无法选择执行器**

- 现象：实际调试时，前端执行器选择入口没有可选择项或无法完成选择。
- 已确认根因：Integration contribution、definition、Host 与按需 offer 创建链已经装配；Agent Runtime cutover 删除旧 discovery routes 后，前端仍请求 `/agents/discovery` 与 `/agents/discovered-options/stream`，请求 404 后投影为空。
- 诊断必须覆盖：Integration contribution 注册、实例与 offer 生成、Driver Host inventory、ProjectAgent/runtime binding、API contract、前端 executor selector。
- 修复后，平台内置 Native/Codex Integration 必须按真实可用性出现在选择入口；不可用项必须展示明确原因，不得伪装为可用或静默消失。
- execution profile 是产品可配置的逻辑执行形态；Runtime offer 是带 provider/model/credential scope/placement 的实例级可运行声明。两者不得混为同一个列表，首次运行前 offer 为空是合法状态。

### R3. LLM Provider 登录认证上下文

**问题 ARD-002：添加 LLM Provider 时提示 ChatGPT OAuth 当前会话未登录**

- 现象：添加 LLM Provider 时直接显示“平台登录状态缺失，ChatGPT OAuth 需要登录当前会话”。
- 已确认根因：桌面 OAuth 客户端在请求发出前强制要求平台 access token，且 Tauri bridge 对空 token 重复拒绝；Personal auth 模式本应允许无 Bearer 请求并由服务端建立 local identity。
- 诊断必须区分平台账户会话、桌面/Codex 登录、ChatGPT OAuth credential、Provider 配置与 Agent Integration credential 的所有权。
- 诊断必须追踪：Provider 创建请求、认证方式默认值、请求身份、credential resolver、provider validation、运行 placement 的凭据解析。
- 修复后的 Provider 创建和验证必须只要求所选认证方式真实需要的凭据；认证缺失时返回可操作、归属明确的状态，不得把不同 credential domain 合并为模糊的“当前会话”。
- 全局 Provider credential 的所有权和运行解析不跟随交互用户；Provider 管理操作仍由 API 的 Personal/Enterprise 授权边界裁决。Personal 模式允许桌面无 token 发起，Enterprise 模式无 token 返回 401，非管理员返回 403。

### R4. 回归归属

- 对每个问题比较 `main`、Agent Runtime PR 分支及必要的持久化状态，确认是代码回归、迁移状态、环境配置还是旧基线问题。
- 属于 Agent Runtime PR 的回归应直接修复在当前分支并更新 PR #93。
- 与本次重构无关的既有问题仍可在本任务登记，但必须明确归属后再决定提交与 PR 边界。

## Acceptance Criteria

- [x] ARD-001 已在 `pnpm dev` 启动的真实产品路径复现并记录第一个断点位置。
- [x] ARD-001 已定位唯一根因，Integration 从 contribution 到前端 selector 的每一跳都有代码或运行证据。
- [x] ARD-001 修复后，Native/Codex execution profile 均由 canonical Host inventory 投影；Codex 可直接选择，PI_AGENT 在缺少 Provider 时展示明确不可用原因并在配置后转为可用。
- [x] ARD-002 已在真实 Provider 创建入口复现，并记录实际提交的认证方式与服务端错误来源。
- [x] ARD-002 已明确平台账户、ChatGPT OAuth、Provider credential 与 Integration credential 的领域边界。
- [x] ARD-002 修复后，Personal 无 token 可创建全局 openai_codex Provider 并成功取得 OAuth flow；Enterprise 认证和管理员权限仍由服务端裁决。
- [x] 每项修复具有对应的最小回归测试和真实 `pnpm dev` 验证。
- [x] 当前已登记的 Agent Runtime PR blocker 在 PR #93 合并前关闭。
- [ ] 后续调试问题能够依照 R1 持续登记，不需要为每次反馈重新创建顶层任务。

## Out of Scope

- 本任务不以关闭 lint 规则、保留旧 Connector/RuntimeSession 路径或添加兼容 fallback 作为调试手段。
- 未经复现和归属判断，不在本任务中顺带修改无关模块。

## Current Issue Register

| ID | 状态 | 严重度 | 问题 |
| --- | --- | --- | --- |
| ARD-001 | verified | blocker | discovery/options 已由 canonical Host definitions 与 Provider catalog 恢复；双 registry 视角已删除 |
| ARD-002 | verified | blocker | 桌面 OAuth token 已改为可选；Personal 无 token 真实 prepare 成功，Enterprise 权限保留 |

## Verification Record

- `pnpm dev` schema 65、API health、local runtime 注册和 Vite 启动通过。
- `GET /api/agents/discovery` 返回 `PI_AGENT` 与 `CODEX`；当前无 Provider 时 PI_AGENT 显示明确原因，CODEX 可用。
- `PI_AGENT` 与 `CODEX` discovered-options NDJSON 均返回 200。
- Personal 无 Authorization 创建临时 `openai_codex` 全局 Provider并调用 desktop OAuth prepare，成功返回 flow ID 与授权 URL；临时 Provider 已删除。
- 最终 Host inventory 的真实 PostgreSQL composition 测试覆盖动态 Native definition，防止 pre-composition registry 再次成为 API 事实源。
- Workspace fmt、check、clippy、contracts、frontend typecheck 与 91 文件/550 项前端测试通过。
