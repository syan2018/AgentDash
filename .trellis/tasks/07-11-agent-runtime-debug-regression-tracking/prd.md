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

### R11. Desktop Local Runtime credential claim 与 Backend 在线收敛

**问题 ARD-009：桌面本机 Runtime 长期停留在 claiming，已选择 Backend 保持离线**

- 现象：本机 Runtime 诊断长期显示“正在领取桌面本机 runtime 凭据”，Runtime action同时返回`目标 Backend 当前不在线: local_029f609c03386a19e4f28779`。
- 已确认事实：该 backend来自当前机器的personal local runtime identity；本机profile启用`auto_start`，但不持久化credential，必须由当前Desktop Dashboard API origin的`POST /api/local-runtime/ensure`重新领取relay credentials并建立`/ws/backend`连接。

**问题 ARD-010：Desktop重启后旧Native binding的outbox命令无限重试**

- 现象：后台持续报告`native binding ... does not exist`，同一outbox命令被反复领取。
- 已确认根因：Native host属于进程内ephemeral execution site，重启后旧binding不可恢复；adapter却把binding缺失报告成普通`Rejected`，outbox因此无法进入Runtime已有的binding-lost收敛路径。
- 验收：binding缺失必须返回typed lost，由Runtime写入canonical `BindingLost`并ack当前outbox命令；线程进入lost后不得继续重派。
- 已确认风险一：desktop ensure使用无请求期限的默认HTTP client，连接或响应读取无法完成时，native supervisor会无限停留在`claiming`。
- 已确认风险二：Web auto-connect把除`error`外的任意`runtimeStart`结果视为成功；`claiming`、`waiting_for_api`、`retrying`尚未证明Backend在线，却会终止前端重试。
- 已确认直接根因：schema 66曾在本机Runtime数据库应用后被原地改名，`agentdash-local`启动迁移因checksum不一致退出；server ensure虽已成功领取同一backend，relay进程从未存活，因此Backend保持offline。
- `claiming`只表达一次有界credential请求正在进行；请求必须在明确期限内成功、进入可诊断重试状态或失败，不能成为无限稳态。
- auto-connect完成的唯一产品事实是native Runtime已经`running`且relay registration已建立；中间态继续由native supervisor推进，前端不得把它们提升为在线成功。
- Backend online admission继续以server-side live registry为准，不对离线backend添加fallback；修复目标是让desktop enrollment/relay真实上线并暴露精确失败证据。

### R12. Shell terminal typed ownership 与续接生命周期

**问题 ARD-011：`shell_exec` start 返回 running 后无法按 terminal_id 续接**

- 现象：真实工具调用中，`shell_exec` start 已返回 `state=running`、`terminal_id` 与 `next_seq`，随后 read 却立即失败为“未找到可续接终端”。本机 shell session 实际仍保留，Agent turn 最终也正常完成。
- 已确认根因：Agent Runtime composition 创建 `VfsRuntimeToolProvider` 时没有注入 shell terminal registry；重构同时删除了 main 上的 terminal registry adapter 与 output snapshot 回写，导致 start 结果虽然暴露 continuation handle，application 侧却没有相同 handle 的 owner/routing 事实。
- `PlatformToolExecutionContext` 是平台工具调用的 canonical typed owner；VFS tool construction 必须把 `run_id`、`agent_id` 与 `runtime_thread_id` 直接传入 shell terminal registration，不再依赖 presentation session 或 UI stream 建立反向绑定后才能续接。
- terminal registry adapter负责把 terminal_id 映射到 backend/mount/cwd 与 AgentRun owner，并记录 start/read/write 的 retained output snapshot；本机 runtime继续拥有真实 process/session buffer，application registry只承担控制路由与产品投影，不复制进程事实。
- start 返回 `state=running` 时，同一 Agent turn 后续 read/write/status/resize/terminate 必须可按 terminal_id 到达原 local runtime session；terminal完成后 retained session在正常保留窗口内仍可读取最终输出。

### R13. Driver event 原子归约与 Runtime revision 稳定性

**问题 ARD-012：Native accepted turn 因 staged revision 冲突终结 Runtime binding**

- 现象：Native turn 已被 Driver 接受并开始事件泵后，Managed Runtime 返回 `expected thread revision RuntimeRevision(19), actual RuntimeRevision(18)`，adapter随即把 binding 终结为 lost。
- 已确认首要根因：Native mapper同时为 Provider retry 状态生成 transient internal `RuntimeEvent::ProviderStatus` 与完整 ephemeral presentation；Managed Runtime契约只接受后者，因此先把正常provider retry误判为critical protocol violation并将Turn收敛为lost。
- 已确认次生根因：Driver envelope按顺序归约多个fact时直接修改工作projection；前置fact已在内存把revision从18推进到19、后置fact transition失败后，violation路径却把这份未提交的staged projection当作committed base，再以19 CAS数据库实际的18，制造确定性的revision conflict并遮蔽原始错误。
- 已确认错误闭环缺口：critical violation虽然持久化了Turn/Operation Lost，却仍向Driver sink返回普通成功，Native Core因此继续运行并再次发送终态；该分支同时绕过正常terminal presentation与application effect outbox，canonical Lost可能无法驱动前端和产品终结副作用。
- `RuntimeRevision`继续作为Thread aggregate、durable journal与projection的事务级CAS坐标；修复不拆分第二事实源，也不放宽CAS。归约必须保留immutable committed base，整批成功才提交staged projection；失败必须从committed base原子写入quarantine、protocol violation与lost terminals。
- ephemeral Provider status/delta只走transient presentation publication，不推进durable revision；Driver adapter不得同时发送语义重复的internal transient fact。
- 验收必须覆盖Provider retry继续运行、混合fact批次后置transition失败的原子回滚、violation持久化不产生revision conflict，以及critical violation明确停止event pump并产生唯一terminal presentation/effect。

### R14. Runtime mutation ownership 与 revision 职责收束

**问题 ARD-013：多producer直接写Thread aggregate，whole-thread revision耦合无关业务更新**

- 已确认现状：Gateway command/driver ingress/presentation与Tool Broker共享单个Runtime实例内mutex；Context/compaction与Hook mutation仍各自执行`load → mutate → commit`。当前production只装配一个Runtime实例，因此这不是ARD-012的直接根因，但跨producer竞争和未来多实例只能依赖数据库CAS，部分内部路径没有安全rebase语义。
- main-reference通过数据库原子递增session event sequence建立较薄的单写入边界；新Runtime引入canonical aggregate后获得Host placement、generation fencing、Operation/outbox、跨Integration lifecycle与恢复能力，但没有同步建立统一mutation owner。
- `EventSequence`只承担durable journal cursor；whole-thread revision只表达aggregate版本和内部CAS，不再作为surface/context/binding等已有专用revision/digest的默认precondition。presentation-only transient不进入aggregate mutation。
- 目标是per-thread mutation coordinator：各producer提交typed intent，从durable base执行纯归约；进程内keyed serialization降低冲突，数据库CAS提供跨实例权威。内部CAS loser只有在具备producer幂等identity时才有界reload/reapply，不得盲重放或把普通竞争升级为binding lost。
- 保留canonical Runtime、Host binding/generation、Operation/outbox与Tool/Hook/Context durable事实；删除不产生恢复或审计收益的presentation internal镜像，不恢复旧Connector/RuntimeSession第二事实源。

### R15. Provider 协议终态与 Mailbox 投递重规划

**问题 ARD-014：可见回复不终结，Promote 后 presentation turn 必然错配**

- 现象：Agent 已输出完整回复后 Runtime 仍长期保持 active；此时将排队消息“立即发送”会失败为 `AgentRun active presentation turn changed`。
- 已确认 Provider 根因：Responses bridge 收到协议终态 `response.completed` 时只更新 usage，仍等待 HTTP body EOF 才发送 `StreamChunk::Done`。真实复现中可见 transient 文本完成后又等待约 154 秒，最终 body decoder 断流才生成失败终态，导致 UI 可见完成与 canonical Message/Turn terminal 分裂。
- 已确认 Mailbox 根因：普通 active-turn submit 预先保存未来 launch 的 presentation input；Promote route 随后只把 delivery/barrier/drain 改成 steer，不重规划 payload，形成 `SteerActiveTurn + future PresentationTurnId` 的不可能状态。即使修正该确定性错配，inspect、claim与facade execute之间的终态竞争仍会使冻结的旧 presentation identity 失败或永久悬空。
- Provider bridge必须以供应商协议的明确 terminal event结束本次逻辑流；收到`response.completed`后完成当前SSE event归约、发送唯一`Done`并停止读取，后续transport EOF或decoder状态不得改写已完成的响应。
- Mailbox只拥有输入草稿、投递策略、顺序、barrier、幂等与claim lifecycle，不生成或持久化canonical Runtime/Presentation identity。AgentRun Runtime facade在带当前snapshot guard的command admission内为start生成稳定identity，或为steer绑定当前active presentation turn。
- Promote必须通过Mailbox application owner修改投递意图，不由API直接拼repository字段。active turn在inspect与execute之间变化时，facade返回typed stale admission；Mailbox保留同一draft重新观察并规划start/steer，不重用旧presentation坐标，也不把合法竞争标记为永久失败。

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
- [x] ARD-006 Native/Codex Hook requirements、failure policy、workspace profile 与实际 execution site 一致，新建和复用 offer 共用同一 Surface admission。
- [x] ARD-006 通过真实 `pnpm dev` Draft create-run 验证 Runtime binding越过 Hook/offer 求交。
- [x] ARD-008 event stream使用统一 `/api` builder，durable replay不重复追加。
- [x] ARD-008 恢复基于当前架构事实的 AgentRun list/workspace product projection，并完成 route-consumer inventory。
- [x] ARD-009 desktop ensure在网络悬挂时有明确超时与诊断状态，不会永久停留`claiming`。
- [x] ARD-009 已应用migration保持immutable，字段改名通过后续migration完成；既有本机Runtime数据库可正常升级并启动。
- [x] ARD-009 auto-connect只在Runtime/relay真实运行后完成，并能从`waiting_for_api`、`retrying`或失败状态继续收敛。
- [x] ARD-009 使用真实产品链验证目标personal backend进入online，Runtime action不再受目标Backend离线admission阻断。
- [x] ARD-009 Desktop native host不依赖Web bridge先提供token即可按profile自动启动Personal runtime。
- [x] ARD-010 ephemeral Native binding缺失会收敛为canonical lost，历史outbox命令完成派发且不再重试。
- [x] ARD-011 `shell_exec` 使用 typed Platform Tool owner 注册 terminal，running start 返回的 terminal_id 可完成后续 read 并取得最终状态/输出。
- [x] ARD-011 恢复 terminal output snapshot 投影，并以 application/VFS/API 定向测试覆盖 start→read 生命周期和 composition 装配。
- [x] ARD-012 Native Provider retry只发布ephemeral presentation，不再触发critical Runtime protocol violation或binding lost。
- [x] ARD-012 Driver fact batch从committed base原子归约；后置fact失败时不提交前置staged mutation，也不再产生staged-vs-database revision conflict。
- [x] ARD-012 critical violation明确terminalize Driver pump，并通过同一terminal projection/effect边界向前端与产品层发布唯一终态。
- [x] ARD-012 通过Runtime interface、Native/Codex/Remote pump与真实embedded PostgreSQL UoW验证retry/error/critical terminal稳定收敛。
- [ ] ARD-012 通过真实`pnpm dev` Provider链验证retry/error/final terminal产品行为。
- [ ] ARD-013 建立完整Runtime mutation producer矩阵，并将command/driver/tool/hook/context/surface收束到per-thread coordinator或明确的可重试work边界。
- [ ] ARD-013 解耦EventSequence、aggregate revision与surface/context/binding专用precondition，覆盖无关presentation推进时的内部更新稳定性。
- [ ] ARD-013 为跨实例driver event引入可持久去重的producer identity后，验证CAS loser可安全reload/reapply且不会重复terminal。
- [ ] ARD-014 Responses bridge在`response.completed`后立即形成唯一canonical MessageEnd/TurnTerminal，且不等待HTTP EOF或被后续decoder错误覆写。
- [ ] ARD-014 Mailbox draft与delivery policy保持正交；Promote和终态竞争下的消息只会steer当前turn或在idle后start下一turn，不产生stale presentation失败或永久悬空。
- [ ] ARD-014 通过application/provider定向竞态测试和真实`pnpm dev`连续两轮输入验证可见回复、canonical终态与mailbox继续消费一致。
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
| ARD-006 | verified | blocker | AgentFrame持久化immutable HookPlan；Runtime按execution site投影requirements，Native/Codex与新建/复用offer共用同一admission |
| ARD-007 | reported | minor | dev server重启期间瞬时 `useContext` null；刷新消失，暂无稳定复现与 stack |
| ARD-008 | verified | blocker | event、workspace/list/detail lineage、composer/context/interaction与退役consumer均按route ledger完成收束并真实验证 |
| ARD-009 | verified | blocker | 恢复immutable migration历史并以0067收敛schema；ensure有界、Web等待relay registered，目标personal backend已真实online |
| ARD-010 | verified | blocker | Native缺失binding返回typed lost；outbox写入BindingLost并ack，历史重试命令已停止 |
| ARD-011 | verified | major | typed owner与terminal registry成为production VFS构造期必需依赖；真实start→read及跨轮retained read通过 |
| ARD-012 | fixed | blocker | Provider status仅保留ephemeral presentation；violation从committed base原子terminalize并停止全部pump，outbox以canonical二次读取决定ack；待真实Provider产品链复验 |
| ARD-013 | diagnosed | major | canonical Runtime带来placement/recovery收益，但mutation owner与revision职责未收束，Context/Hook及跨实例CAS仍存在结构性竞争面 |
| ARD-014 | diagnosed | blocker | Responses等待HTTP EOF导致可见回复与canonical终态分裂；Promote跨层改policy却保留预生成presentation坐标，立即发送确定性失败 |

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
- 修正 event URL与durable-only replay后，重新打开同一 AgentRun只显示一份`runtime-ok`；该阶段workspace/list projection仍因cutover route缺失保持ARD-008 open。
- ARD-008 foundation新增`AgentRunProductQuery`与`AgentRunProductView`，恢复`GET /agent-runs/{run}/agents/{agent}/workspace`；产品投影与Runtime inspect使用独立settle/error state，refresh单路失败保留各owner上一份成功事实。
- 真实Draft `3240bb88-bbf8-42eb-ba8a-1fc883685e9a` / agent `1a12a893-eed8-4e99-be9f-c0bf3defebe4` 返回`foundation-ok`；workspace投影包含resolved `gpt-5.4-mini`、current frame与default mount `main`，独立Runtime snapshot为active revision 10，浏览器无error日志。侧栏Project AgentRun list仍因缺少route显示Not Found，按route ledger留待下一切片。
- ARD-008 foundation定向质量门通过：`AgentRunProductQuery` 3项模型投影测试、前端workspace/command/control/compaction 26项测试、app-web typecheck、相关ESLint、contracts check及application/agentrun/API `-D warnings` clippy；删除无消费者的`AgentRunWorkspaceView`与旧command precondition wire合同。
- ARD-008 list切片新增`ProjectAgentRunListQuery`，以LifecycleRun/LifecycleAgent、ProjectAgent identity、subject association、canonical `AgentLineage` forest与Managed Runtime inspect重建`GET /projects/{project_id}/agent-runs`；新generated wire删除退役workspace shell、delivery/run/frame副本，仅保留当前列表consumer读取的产品与Runtime摘要事实。
- 真实产品验证中，侧栏与Agent Hub均恢复6条AgentRun，最新行导航到`3240bb88-bbf8-42eb-ba8a-1fc883685e9a` / `1a12a893-eed8-4e99-be9f-c0bf3defebe4`，详情继续显示`foundation-ok`且浏览器error日志为空。route ledger同时补记仍缺owner的delete/detail lineage/mailbox/journal/fork，ARD-008保持fixed直到剩余cutover项分别完成。
- `cargo test -p agentdash-api agent_runtime_surface::tests` 4项、`cargo test -p agentdash-integration-native-agent` 11项、runtimeEventStream 2项与app-web typecheck通过；目标三crate `--lib` clippy通过。`--all-targets`另暴露`agentdash-agent-runtime-test-support`既有`collapsible_match`，未修改该无关文件。
- ARD-007 review确认全仓React/ReactDOM均为19.2.4且解析到同一物理文件，Vite预构建只有唯一React source，Draft相关定向ESLint通过，Canvas React 18位于隔离iframe。当前缺少稳定复现、error stack/componentStack与module URL，因此保持`reported`并阻止alias/dedupe、try/catch、强制reload或双React兼容补丁。
- schema 66新增`agent_frames.hook_plan jsonb`；AgentFrame construction统一编译immutable HookPlan，Runtime只将Driver/AgentCoreCallback execution site投影到Driver surface。最终真实Run `48e6b105-37da-47e7-bff4-246ec7dfca88`使用空HookPlan成功binding，证明不再要求offer伪造`BeforeTool`能力。
- 首轮/第二轮连续会话分别在真实Run中返回`first-turn-ok`与`second-turn-ok`。由此定位并修复Managed Runtime与Driver重复创建Turn identity：`TurnStart`现在唯一拥有canonical Runtime turn，Driver source turn独立映射，matching `TurnStarted`只作为acknowledgement。
- 工具调用产生业务失败后，Runtime保持active且0 protocol violation；后续同一Run成功返回`after-tool-ok`。最终snapshot revision 29，2个started/2个terminal，证明terminal command已完成outbox ack且未被重复派发。
- 最终产品验证覆盖composer、Runtime context、generic interaction availability与工具后follow-up；浏览器error日志为空，未复现`useContext`异常。ARD-007仍按独立证据边界保持reported。
- 质量门通过：migration guard、contracts generation/check、app-web typecheck、87文件/451项前端测试、changed-files ESLint、workspace `cargo check --all-targets`、相关Rust tests与目标crate strict clippy。全仓frontend lint仍命中32个本次未改文件中的既有`react-hooks/set-state-in-effect`错误，changed-files lint为0。
- ARD-009真实复现证明server ensure已经领取`local_029f609c03386a19e4f28779`，但`agentdash-local`因已应用migration 66被原地修改而退出；Backend offline是relay进程未存活的结果，不是Backend registry admission误判。
- 0066恢复原始checksum并新增0067顺序rename；当前Dashboard开发库完成一次migration metadata修复以保留既有业务数据，本机Runtime数据库随后从原始0066成功升级至schema 67。重新执行`pnpm dev`后同一backend完成relay registration，API返回`online=true`且产品“后端连接”显示`dev-local / 项目同步 · 已连接`。
- Desktop credential claim增加5秒connect/15秒request deadline，timeout进入retryable `desktop_claim_timeout`；Web auto-connect使用complete/pending/inactive三态，仅`running + relay registered + backend identity一致`完成。Rust 9项与前端8项定向测试通过。
- ARD-011定向验证通过：四个相关crate `cargo check`、VFS running start→write/read生命周期测试、API typed owner/binding与增量snapshot两项测试、workspace fmt与diff check均通过。strict no-deps clippy执行后仅命中既有`apply_patch.rs:246 collapsible_if`与`shell.rs:982 too_many_arguments`，未改无关告警。
- ARD-011真实`pnpm dev` Run `689ea30d-cdf2-4e1b-aac9-613dc72de8ed` / agent `649e80d6-bd33-4337-929e-f0856fae58f9`：`main://` start返回`running`、terminal `term-1784122916605-a9eb7028`、`next_seq=0`；同轮read返回`completed`、exit 0、`next_seq=4`，下一用户轮不带cursor读取同一已完成terminal仍取得`shell-start`与`shell-done`完整retained output。数据库同时保留旧terminal的“未找到可续接终端”失败与新terminal三条completed记录，完成同库前后对照。
- ARD-012真实embedded PostgreSQL回归在Runtime UoW末阶段注入失败，证明projection/journal/operation/effect/quarantine全部回滚；重试从committed base提交唯一ProtocolViolation、Lost、terminal presentation/effect/quarantine，且thread column revision、projection revision与journal MAX(revision)一致，event head一致。
- ARD-012 fabricated `DriverError::Terminalized`真实outbox claim回归证明canonical operation仍active时release/no-ack且可reclaim，claim期间canonical terminal后二次读取才ack。Runtime interface 44、Native 37+16、Codex 50、Remote 19、相关crate check、contracts、fmt与diff check通过；本轮未启动`pnpm dev`，真实Provider产品链仍待复验。
