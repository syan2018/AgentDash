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

### R5. AgentRun 启动配置与 placement

**问题 ARD-003：Runtime 拒绝单次启动模型配置与 backend selection**

- 现象：从 AgentRun Draft 选择 executor/provider/model/backend 并启动时，API 返回“当前 Runtime surface 不接受单次启动 executor/backend override”。
- 已确认根因：ProjectAgent create-run contract 仍暴露合法的启动选择，WP08 route 却整体拒绝；同时新的 Runtime admission 没有把选择写入 AgentFrame execution profile 和 provision request。
- ProjectAgent 决定 executor/Integration identity；AgentRun 不切换 executor，可以覆盖该 executor 接受的 Provider、模型与其他运行参数，并选择 backend placement。
- effective model config 必须在 Runtime provision 前持久化到 AgentFrame revision；backend selection 必须进入 Host offer selection并最终由 Runtime binding记录实际 placement。
- 启动 override 不得只在 HTTP 调用内临时生效，也不得更新 ProjectAgent 默认配置。

### R6. AgentRun 启动前的 AgentFrame VFS surface

**问题 ARD-004：真实 ProjectAgent 会话在 Runtime binding 阶段缺少 VFS default mount**

- 现象：从真实 ProjectAgent Draft 开始会话时返回 `AgentRun runtime binding is unavailable: AgentRun VFS has no usable default mount`。
- 已确认根因：Lifecycle launch 只通过 `AgentRunLaunchAnchorFrameConstructionAdapter` 创建带 execution profile 的 launch-anchor AgentFrame，没有在 Runtime provision 前写入 Project workspace/VFS/capability surface；`BusinessFrameSurfaceQuery` 随后把缺失 VFS 投影为空 VFS，`AgentFrameNativeSurfaceCompiler` 因不存在可用 default mount 拒绝绑定。旧 runtime-session owner bootstrap 曾在更晚的 connector launch 阶段补齐该 surface，WP08 cutover 删除旧启动链后没有把 surface materialization 前移。
- ProjectAgent 首次启动必须在 Runtime provision 前基于 Project、ProjectAgent、workspace、Project VFS mounts 与现有 capability sources 生成完整 AgentFrame revision；Business Surface compiler 只消费该持久事实，不临时构造 workspace cwd。
- VFS default mount 必须来自 canonical Project/workspace mount resolution，并保留 backend/root/provider/capability 坐标；不得使用进程 cwd、空目录、任意在线 backend 或静默 fallback。
- 缺少真实 workspace/mount 时应在 frame materialization/admission 边界返回归属明确的 typed error，不进入 Driver Host binding。

### R7. Runtime tool 的 capability ownership

**问题 ARD-005：完整 VFS surface 进入 Runtime compiler 后无法解析 `mounts_list` capability**

- 现象：ARD-004 修复后真实 Draft 已越过 default mount 校验，但 Runtime binding 返回 `assembled tool mounts_list has no unambiguous AgentFrame capability identity`。
- 已确认根因：`CapabilityState.tool_policy` 按契约是只保存 whitelist/exclude 的稀疏运行策略；Runtime surface compiler 却把它误当作完整 tool-to-capability registry。平台工具的 canonical ownership 已由 `platform_tool_descriptors()` 定义，`mounts_list` 属于 `file_read`。
- Runtime surface compiler 必须从 canonical tool descriptor 读取 capability key，并通过当前 `CapabilityState::is_capability_tool_enabled` 校验 capability/cluster/policy；未知、未授权或真正歧义的工具继续在 Host side effect 前严格拒绝。
- 不得通过“当前只有一个 capability”、任意首项或给所有工具补空 policy 的方式猜测 ownership。

### R8. Business Surface 与 Runtime offer 的单一求交边界

**问题 ARD-006：Runtime compiler 硬编码的 Hook 要求与 offer 能力证明矛盾**

- 现象：ARD-005 修复后真实 Draft 继续进入 Host binding，但返回 `required hook BeforeTool is not guaranteed by the selected offer`。
- 已确认根因：Runtime surface compiler 无条件构造 `BeforeTool/AfterTool` binding，其中 `BeforeTool.EmitEffect` 不属于 Native/Codex driver callback 的必要动作，`AfterTool` 的 fail-open 要求又与 Native 仅声明 fail-closed 的实现不一致；首次动态创建 offer 后还绕过了复用路径已有的 `offer_supports_surface` 求交。
- Hook action 必须按唯一 execution site 表达：Driver profile 只证明 driver/native callback 真正需要承担的同步语义，Managed Runtime 持久化 effect 不得伪装成 driver action requirement。
- Runtime offer 无论来自既有 inventory 还是本次动态 activation，都必须经过同一个完整 Surface admission；Host bind 只接受已经求交成功的 immutable offer/surface。
- Native 的 `HostAdaptedExact` workspace 与 Hook failure policy 必须由实现和 conformance profile 同源声明，不得用空 profile 或 Host 侧宽松判断掩盖。
- 结构性收口以 AgentFrame revision 持有 immutable HookPlan ref/digest/requirements 为终态；Runtime compiler 消费该事实，不继续维护无来源的固定 Hook 列表。

### R9. Draft 页面瞬时 React Context 异常

**问题 ARD-007：开发服务器重启期间偶发 `useContext` null**

- 现象：保持 Draft 页面打开并重启 `pnpm dev` 后，页面曾瞬时显示 `Cannot read properties of null (reading 'useContext')`，手动刷新后消失。
- 当前证据：workspace 的 React/ReactDOM 均解析为 19.2.4，尚无稳定的多 React 实例或产品构建复现证据；该问题先保留为 `reported`，与稳定的 Runtime binding blocker 分开跟踪。
- 后续只有在可重复出现并取得浏览器 stack/module URL 后才修改前端依赖或 Vite 配置，避免把开发期 HMR 断连现象误判为产品代码根因。

### R10. AgentRun API cutover 与事件消费语义

**问题 ARD-008：Runtime 成功运行后 Workspace projection 404，transient event 被重复回放**

- 现象：真实 AgentRun 已 active 并完成 `runtime-ok`，运行页却显示 event stream HTTP 404、Not Found与模型缺失；修正路径后，同一 transient delta 在有限批次结束后的自动重连中反复追加。
- 已确认根因一：NDJSON 客户端直接使用 origin resolver，漏掉统一 API builder注入的 `/api` 前缀。
- 已确认根因二：重构分支把 `lifecycle_agents` 收缩为 Runtime command routes时，删除了前端仍消费的 Project AgentRun list与`AgentRunWorkspaceView` projection；`useAgentRunWorkspaceState` 又用一个 `Promise.all` 同时加载 workspace与runtime inspect，workspace 404使已成功的 Runtime snapshot也被丢弃。
- 已确认根因三：Runtime `events()` 当前是有限 replay batch，transient event没有 durable cursor且repository永久保留；客户端把批次结束当断线重连会再次读取同一 transient history。
- cutover必须建立“前端调用 → route → application owner → contract test”的完整 inventory。每个旧入口只能被明确迁移到新 owner、替换消费者或连同契约一起删除，禁止用文件级替换让 route静默消失。
- `AgentRunWorkspaceView` 应由当前 Lifecycle/AgentFrame/Managed Runtime事实重建为产品 projection，不恢复已退役 RuntimeSession作为执行事实源；Runtime inspect与workspace projection的加载失败必须各自归属，不互相抹掉已成功事实。
- 在 Runtime 提供真正 live subscription或稳定 transient identity/cursor前，有限轮询只消费 durable events；不能以易误删合法重复 chunk的文本指纹去重伪造 exactly-once。

## Acceptance Criteria

- [x] ARD-001 已在 `pnpm dev` 启动的真实产品路径复现并记录第一个断点位置。
- [x] ARD-001 已定位唯一根因，Integration 从 contribution 到前端 selector 的每一跳都有代码或运行证据。
- [x] ARD-001 修复后，Native/Codex execution profile 均由 canonical Host inventory 投影；Codex 可直接选择，PI_AGENT 在缺少 Provider 时展示明确不可用原因并在配置后转为可用。
- [x] ARD-002 已在真实 Provider 创建入口复现，并记录实际提交的认证方式与服务端错误来源。
- [x] ARD-002 已明确平台账户、ChatGPT OAuth、Provider credential 与 Integration credential 的领域边界。
- [x] ARD-002 修复后，Personal 无 token 可创建全局 openai_codex Provider 并成功取得 OAuth flow；Enterprise 认证和管理员权限仍由服务端裁决。
- [x] 每项修复具有对应的最小回归测试和真实 `pnpm dev` 验证。
- [x] 当前已登记的 Agent Runtime PR blocker 在 PR #93 合并前关闭。
- [x] ARD-003 启动时选择的 Provider/model 被写入 AgentFrame effective execution profile；executor 始终继承 ProjectAgent 并驱动对应 Integration definition/service instance。
- [x] ARD-003 explicit backend 只匹配目标 backend 的 activated Runtime offer；无匹配 offer返回精确 unavailable error。
- [x] ARD-003 通过真实 Draft create-run 验证 override 已穿过 API、Lifecycle 与 Runtime surface compiler；空测试项目随后因缺少 VFS mount 被独立拒绝。
- [x] ARD-004 真实 ProjectAgent Draft 在 Runtime provision 前持久化包含 canonical default mount 的完整 AgentFrame Business Surface。
- [x] ARD-004 通过定向回归测试与真实 `pnpm dev` create-run验证越过 VFS default mount 断点并进入后续 tool surface compilation。
- [x] ARD-005 canonical platform tool descriptor 能为 `mounts_list` 等 assembled tools 提供唯一 capability ownership，并保持 capability policy admission。
- [x] ARD-005 通过真实 `pnpm dev` Draft create-run 验证 Runtime binding继续越过 tool surface compilation。
- [ ] ARD-006 Native/Codex Hook requirements、failure policy、workspace profile 与实际 execution site 一致，新建和复用 offer 共用同一 Surface admission。
- [x] ARD-006 通过真实 `pnpm dev` Draft create-run 验证 Runtime binding越过 Hook/offer 求交。
- [x] ARD-008 event stream使用统一 `/api` builder，durable replay不重复追加。
- [ ] ARD-008 恢复基于当前架构事实的 AgentRun list/workspace product projection，并完成 route-consumer inventory。
- [ ] 后续调试问题能够依照 R1 持续登记，不需要为每次反馈重新创建顶层任务。

## Out of Scope

- 本任务不以关闭 lint 规则、保留旧 Connector/RuntimeSession 路径或添加兼容 fallback 作为调试手段。
- 未经复现和归属判断，不在本任务中顺带修改无关模块。

## Current Issue Register

| ID | 状态 | 严重度 | 问题 |
| --- | --- | --- | --- |
| ARD-001 | verified | blocker | discovery/options 已由 canonical Host definitions 与 Provider catalog 恢复；双 registry 视角已删除 |
| ARD-002 | verified | blocker | 桌面 OAuth token 已改为可选；Personal 无 token 真实 prepare 成功，Enterprise 权限保留 |
| ARD-003 | verified | blocker | RunLaunchProfile 已进入 AgentFrame、Integration definition 与 backend offer selection |
| ARD-004 | verified | blocker | ProjectAgent launch 在 product delivery 前物化完整 owner surface；真实 Draft 已越过 VFS default mount 断点 |
| ARD-005 | verified | blocker | canonical descriptor 已解析 `mounts_list`；真实 Draft越过 tool surface并完成回复 |
| ARD-006 | fixed | blocker | Native action/failure/workspace与新建offer admission已验证；Codex需由真实HookPlan route消除固定`RequestApproval`要求 |
| ARD-007 | reported | minor | dev server重启期间瞬时 `useContext` null；刷新消失，暂无稳定复现与 stack |
| ARD-008 | diagnosed | blocker | Runtime成功但 API cutover删除 workspace/list projection；event path与transient replay语义另有断链 |

## Verification Record

- `pnpm dev` schema 65、API health、local runtime 注册和 Vite 启动通过。
- `GET /api/agents/discovery` 返回 `PI_AGENT` 与 `CODEX`；当前无 Provider 时 PI_AGENT 显示明确原因，CODEX 可用。
- `PI_AGENT` 与 `CODEX` discovered-options NDJSON 均返回 200。
- Personal 无 Authorization 创建临时 `openai_codex` 全局 Provider并调用 desktop OAuth prepare，成功返回 flow ID 与授权 URL；临时 Provider 已删除。
- 最终 Host inventory 的真实 PostgreSQL composition 测试覆盖动态 Native definition，防止 pre-composition registry 再次成为 API 事实源。
- Workspace fmt、check、clippy、contracts、frontend typecheck 与 91 文件/550 项前端测试通过。
- ARD-003 真实 create-run 不再返回 override 拒绝；请求中的 `CODEX + explicit local backend` 已进入 Runtime surface compiler。临时空 Project 因没有默认 VFS mount在后续 surface 编译阶段失败，测试 Project 已删除。
- ARD-004 embedded PostgreSQL Lifecycle launch正例证明 current AgentFrame 在 product delivery 前已包含 canonical workspace mount、backend/root/workspace binding、capability/context 与逐 Run execution profile；无 workspace负例在 frame construction 精确失败。
- ARD-004 真实 `pnpm dev` Draft 已不再返回 `AgentRun VFS has no usable default mount`，随后在独立的 tool capability ownership 断点停止。
- ARD-005/006 真实 Draft `2a977413-7aa3-48d2-b4f6-141eb6046ca9` 建立 active Native binding，HookPlan applied，模型回复 `runtime-ok`，Runtime snapshot revision 10且10条 durable event可读取。
- 修正 event URL与durable-only replay后，重新打开同一 AgentRun只显示一份`runtime-ok`；workspace/list projection仍因cutover route缺失保持ARD-008 open。
- `cargo test -p agentdash-api agent_runtime_surface::tests` 4项、`cargo test -p agentdash-integration-native-agent` 11项、runtimeEventStream 2项与app-web typecheck通过；目标三crate `--lib` clippy通过。`--all-targets`另暴露`agentdash-agent-runtime-test-support`既有`collapsible_match`，未修改该无关文件。
- ARD-007 review确认全仓React/ReactDOM均为19.2.4且解析到同一物理文件，Vite预构建只有唯一React source，Draft相关定向ESLint通过，Canvas React 18位于隔离iframe。当前缺少稳定复现、error stack/componentStack与module URL，因此保持`reported`并阻止alias/dedupe、try/catch、强制reload或双React兼容补丁。
