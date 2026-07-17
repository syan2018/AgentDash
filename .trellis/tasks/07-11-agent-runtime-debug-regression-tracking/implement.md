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

## ARD-004

- [x] 将 ProjectAgent launch-anchor construction 接入 canonical Project/workspace/VFS owner surface materialization，并在 Runtime provision 前持久化完整 AgentFrame revision。
- [x] 保持 effective execution profile 与本次 Run 选择写入同一最终 surface revision，不在 Runtime compiler 内构造临时 cwd/VFS。
- [x] 补充真实 Project/workspace mount fixture，证明 current AgentFrame 具有可用 default mount且 Runtime surface compiler可以继续绑定。
- [x] 覆盖无可用 Project workspace/mount 的精确失败语义，确认不回退进程 cwd、任意 backend或空 mount。
- [x] 运行相关 Rust 定向测试、fmt/check/clippy，并使用 `pnpm dev` 验证真实 Draft越过 VFS default mount 断点。

## ARD-005

- [x] Runtime surface compiler从 canonical platform tool descriptor解析 assembled tool 的 capability key/cluster。
- [x] 使用 current AgentFrame `CapabilityState` 校验 capability与稀疏 tool policy，不从 policy 表缺失推断 ownership。
- [x] 补充 `mounts_list -> file_read`、policy exclude与未知 tool strict rejection 定向测试。
- [x] 优先使用真实 `pnpm dev` Draft create-run验证下一个产品断点；通过后再运行完整质量检查。

## ARD-006

- [x] 删除 Driver `BeforeTool` requirement 中属于 Managed Runtime 的 effect action，保留真实同步 callback语义。
- [x] 让 Native Hook capability、apply acknowledgment与 bound failure policy由同一 profile事实驱动，并支持 `AfterTool` fail-open。
- [x] 如实声明 Native `HostAdaptedExact` workspace能力，避免完整 VFS surface与空 profile矛盾。
- [x] 新 activation offer与既有 offer复用同一个 Surface admission，拒绝未经求交的 Host bind。
- [x] 在AgentFrame construction编译并持久化immutable HookPlan revision/ref/digest/requirements，所有writer共用同一编译入口。
- [x] Runtime materializer按execution site只投影Driver/AgentCoreCallback routes，Managed Runtime/ToolBroker routes不再进入Driver要求。
- [x] 使用真实 `pnpm dev` Draft create-run优先验证产品链路；成功后再补定向回归与完整质量检查。

## ARD-008

- [x] Runtime NDJSON客户端改用统一 API path builder，修复遗漏`/api`导致的永久404。
- [x] 有限event replay只请求durable events，停止无cursor transient history在轮询重连中重复追加。
- [x] 建立AgentRun前端service/route/application owner/generated contract inventory，分类迁移、替换与删除项。
- [x] 以`AgentRunProductQuery`组合Lifecycle、current AgentFrame/model config与VFS surface，恢复workspace detail产品投影且不恢复旧RuntimeSession执行链。
- [x] 拆分workspace与runtime inspect加载错误归属，验证Runtime成功事实不会被另一projection失败清空。
- [x] 从Lifecycle与canonical Runtime summary重建Project AgentRun list projection，关闭侧栏Not Found。
- [x] 将context读取与interaction response切换到generated Runtime generic contracts，删除旧projection与专用interaction分支。
- [x] 删除无canonical owner的delete入口及legacy mailbox/journal consumer，Runtime feed成为唯一事件投影入口。
- [x] detail projection直接读取canonical lineage，并与Project list共用title与递归深度规则。
- [x] 让Runtime lifecycle事件统一失效runtime inspect，保持turn/interaction availability及时刷新。

## Runtime lifecycle convergence

- [x] `TurnStart`由Managed Runtime唯一创建canonical Turn，并通过Driver command envelope下传identity。
- [x] Native/Codex adapter分离canonical Runtime turn与Integration source turn，Tool/Hook/terminal均引用前者。
- [x] matching `Driver TurnStarted`作为acknowledgement接纳，不创建第二个Turn或推进revision/cursor。
- [x] Driver已经发出terminal后，底层任务的同一失败按成功dispatch完成outbox ack，阻止终态command重派。

## ARD-009

- [x] 恢复已应用0066 migration内容并新增0067 rename migration，验证既有本机Runtime数据库可顺序升级。
- [x] 为desktop ensure HTTP client设置有界connect/request timeout，并保持timeout为可重试typed claim error。
- [x] 修正Web auto-connect completion判定：只有`running`且relay为`registered`完成；中间态只观察snapshot、不重复start，也不吞掉后续收敛机会。
- [x] 覆盖HTTP悬挂、waiting/retrying中间态与最终running的Rust/前端定向测试。
- [x] 用真实`pnpm dev`检查credential ensure、既有本机数据库升级、relay registration与server Backend online事实。
- [x] 在目标Backend online后确认产品连接投影为已连接，offline Runtime action admission条件已消失。
- [x] 修正Desktop native host的auto-start所有权：profile允许自动启动时由native直接执行ensure，Web bridge只在具备用户token时补充认证上下文。
- [x] 真实`pnpm dev:desktop`验证无Web token的Personal profile自动建立relay，目标Backend进入online。

## ARD-010

- [x] Native ephemeral host找不到旧binding时返回typed `DriverError::Lost`，不再伪装为可重试的命令拒绝。
- [x] Runtime outbox将binding lost投影为canonical `RuntimeEvent::BindingLost`并完成当前命令ack。
- [x] 真实历史命令从8614次重复派发收敛为`dispatched_at`，线程状态进入`lost`且后台停止刷错。

## ARD-011

- [x] 从`PlatformToolExecutionContext`向VFS tool construction传递typed `run_id`、`agent_id`与`runtime_thread_id`，由shell start registration直接持有canonical owner。
- [x] 在API composition恢复`AgentRunTerminalRegistry` adapter并注入`VfsRuntimeToolProvider`；control operation按terminal_id解析backend/mount/cwd，不依赖前端订阅建立session binding。
- [x] 恢复start/read/write output snapshot回写，保持application terminal projection与local retained buffer职责分离。
- [x] 增加VFS tool start→read生命周期测试和production composition装配测试，确认running handle可续接、completed retained output可读取。
- [x] 运行目标Rust测试、fmt、check/clippy，并以最近数据库复现参数验证真实`pnpm dev`链路。

## ARD-012

- [x] 删除Native adapter对Provider retry状态的internal transient重复投影，保留完整ephemeral Backbone presentation并补mapper回归。
- [x] 将Driver envelope ingestion改为committed base + staged projection归约；所有transition/protocol violation分支从committed base提交，不消费未落库revision。
- [x] 增加“前置fact可归约、后置fact失败”的Runtime interface与embedded PostgreSQL测试，断言violation/lost/quarantine原子落地且无revision conflict、无前置partial fact。
- [x] 让critical violation admission明确停止Native/Codex/Remote/durable worker event pump，并复用terminal presentation/application effect构建，断言只有一个对外终态且不追加第二份BindingLost。
- [x] 让outbox在dispatch error后回读canonical状态；fabricated `Terminalized`遇到active operation必须release/no-ack，只有canonical terminal/obsolete才ack。
- [x] 复核Driver event、Tool Broker、Context/Hook worker的revision producer与锁/CAS边界，固定ARD-013的mutation owner与revision职责收束方向。
- [x] 运行Runtime、Native、Codex、Remote、PostgreSQL定向测试、相关crate check、contracts、fmt与diff check。
- [ ] 以真实`pnpm dev` Provider retry/error/final terminal产品路径确认Turn继续到terminal而非binding lost。

## ARD-013

- [ ] 枚举所有Thread aggregate writer、调用入口、现有锁、work lease、幂等identity与业务precondition，固定唯一mutation ownership矩阵。
- [ ] 建立keyed per-thread mutation coordinator和typed intent入口，迁移command、driver、Tool Broker、Hook、Context与surface内部writer；外部I/O不得持有数据库事务或mutation guard。
- [ ] 将presentation transient完全留在live publication；EventSequence只做journal cursor，aggregate revision与surface/context/binding专用revision各自承担明确职责。
- [ ] 为driver source event建立可持久去重identity，并覆盖双Runtime实例共享PostgreSQL时的CAS reload/reapply、terminal exact-once和column/projection/journal一致性。
- [ ] 对照main-reference运行前端eventstream payload等价矩阵，并验证新架构的placement/rebind/outbox/recovery收益未因收束而退化。

## ARD-014

- [ ] 让Responses stream parser显式识别协议terminal；`response.completed`归约usage后立即发送唯一`Done`并结束读取，覆盖terminal后transport挂起/decoder失败的回归。
- [ ] 将Mailbox stored command中的预生成presentation input替换为typed presentation draft；移除Mailbox对canonical PresentationTurn/Item identity的生成职责。
- [ ] 由AgentRun Runtime facade在start/steer command admission内基于同一canonical snapshot生成或绑定presentation identity，并保持client command幂等。
- [ ] 将mailbox Promote从API直接repository更新迁入Mailbox application owner；policy变更不触碰draft，claim按当前Runtime状态选择start或steer。
- [ ] 覆盖Promote active steer、terminal-before-claim转next start、inspect/execute stale后重规划、无永久failed/queued悬空，以及eventstream payload不变。
- [ ] 运行Provider、Agent、AgentRun application/API/PostgreSQL定向测试与相关fmt/check；最后用真实`pnpm dev`连续提交和Promote验证可见回复立即terminal且下一消息继续消费。
- [x] 将Message Submission receipt/UoW从Mailbox queue拆出，并让Mailbox仅依赖窄delivery settlement port；attached receipt与mailbox settlement保持同事务。
- [x] 以稳定semantic digest在mutable stale guard前reserve/replay composer submission；新命令校验失败条件释放未attach reservation。
- [x] 将typed execution profile override放入隐藏delivery draft并纳入Runtime digest；Steered忽略，Started由Managed Runtime内`ExecutionProfileCoordinator`处理。
- [ ] 引入planned service rebind能力，使不同current Frame execution profile可以原子创建revision、重配service instance并重新对齐binding/snapshot；在此之前保持typed unsupported。
- [ ] 恢复`ProjectAgentRunStartService`具名application owner，覆盖post-launch失败无orphan、duplicate不重复launch以及首次/duplicate错误结果一致。
- [ ] 以AgentDash-owned Codex标准`UserInputBlock`替换Runtime自定义Text/Image/FileReference镜像，覆盖nullable detail、text_elements、LocalImage/Skill/Mention保真、空白拒绝与Native typed unsupported。

## ARD-015

- [x] 以三版本回归固定失败边界：binding bootstrap为F1、Managed Runtime snapshot adopted为F2、repository latest为F3，活动查询必须只返回F2。
- [x] 将exact candidate Frame坐标加入surface compiler/source contract；compiler按F2加载一次完整facts，不再从binding或`AgentFrameRepository::get_current()`重新发现目标。
- [x] 将current adopted Runtime Surface读取收敛到Managed Runtime snapshot/AgentRun facade；删除`BusinessFrameSurfaceQuery`从immutable binding surface推断live adopted Frame的路径。
- [x] 删除`AgentBusinessSurfaceSource`冗余Frame repository依赖与revision equality guard；Context facts复用同一次surface projection返回的exact Frame。
- [x] 将AgentFrame最高revision API按latest persisted真实语义统一改名为`get_latest`，迁移所有调用方且不保留兼容方法。
- [x] candidate持久化前校验自身closure，持久化后按exact Frame完成完整Business Surface/presentation compile，并以expected revision/digest执行`SurfaceAdopt`；只有Managed Runtime snapshot commit能推进active head。
- [x] 覆盖三版本active head、exact candidate compiler、Canvas visibility no-op、surface CAS/失败保持旧snapshot、recovery exact descriptor与production composition工具继续执行。
- [x] 运行相关Rust direct fmt、定向测试、目标三crate check与静态搜索；记录strict clippy中本次未修改的既有lint债。
- [ ] 使用`pnpm dev`真实Canvas create/write/present确认当前turn继续且后续对话可用。

## ARD-016

- [x] 以真实Runtime event/transcript确认Canvas已创建且resource surface已adopt，但两个ToolCall Item因旧tool-set revision fence无法terminal，最终触发active-item protocol violation与Runtime Lost。
- [x] 将`ManagedRuntimeToolJournal`拆为strict admission lookup与accepted-call bound lookup；仅首次accept校验current tool-set revision，progress/approval/terminal保留binding generation及persisted Item校验。
- [x] 统一Broker失败结果构造，为executor failure、timeout、cancel与policy denial写入typed diagnostic `content_items`。
- [x] 删除Workspace Panel的`runtimeStatus === ready` Canvas可见性门禁，改为Workspace Module catalog与durable resource surface求交。
- [x] 增加Surface hot-replace生命周期、typed failure diagnostic和Runtime Lost资源可打开回归测试。
- [x] 删除normalized assignment的Frame revision，并将tool schema source收敛为稳定owner layer，覆盖跨Frame语义不变回归。
- [x] 删除`present -> Canvas visibility -> AgentFrame update`路径；presentation producer仅提交canonical Runtime turn/item并由Runtime补全presentation turn。
- [x] 将Workspace Module create/present typed content改为分行可读摘要，结构化details保持机器可读。
- [x] 修正Native presentation的Broker envelope lossy投影，优先恢复typed `content_items`并增加防单行JSON回归。
- [x] 删除Native AfterTool把结构化result同时写入content的隐式路径；仅显式typed content可覆盖executor正文。
- [x] 增加Canvas present零surface mutation、presentation identity normalization与normalized delta回归。
- [x] 运行Tool Broker、Runtime projection、Workspace Module、Native Integration、Canvas前端测试、typecheck/lint与diff检查。
- [x] 使用`pnpm dev`真实Canvas create/write/present确认工具唯一terminal、用户可打开Canvas且后续对话不进入Lost。

## ARD-017

- [x] 增加回归测试：AgentRun workspace projection必须包含current VFS已挂载且仍有backing asset的Canvas，删除asset后该项消失。
- [x] 增加回归测试：live `ContextFrameChanged`进入通用workspace refresh；presentation必须等待refresh完成。
- [x] 增加回归测试：菜单与presentation共同消费AgentRun `workspace_modules`；旧Project catalog缓存不能影响列表，历史presentation不能打开current projection不存在的URI。
- [x] 将Workspace Module visibility resolver输入收敛为visibility dimension与canonical VFS，并复用于AgentRun workspace query。
- [x] 将generated `AgentRunWorkspaceView`扩展为携带runtime-scoped `workspace_modules`，同步API mapper与TypeScript contract。
- [x] 删除AgentRun WorkspacePanel对Project Workspace Module store及前端resource-surface二次求交的依赖。
- [x] 删除AgentFrame/Runtime Surface/Platform Tool owner中的Canvas/module ref副本及VFS重建入口，并以migration删除数据库列和surface JSON key。
- [x] 以current `workspace_modules`清理持久化Canvas tab，并阻止更早发起的异步layout restore覆盖currentness校验。
- [x] 将`ContextFrameChanged`接入页面control-plane invalidation；presentation执行改为refresh后current projection校验。
- [x] 运行定向Rust、contract generation/check、前端测试/typecheck/lint及真实`pnpm dev`产品链验证。
