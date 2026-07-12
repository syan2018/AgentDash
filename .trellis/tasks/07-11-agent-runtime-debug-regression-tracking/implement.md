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
