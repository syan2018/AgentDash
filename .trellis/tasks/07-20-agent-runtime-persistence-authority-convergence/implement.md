# Agent Runtime 持久化职责与事实边界清理实施进展

## 权威与持久化

- [x] Product 只持久化 LifecycleRun/LifecycleAgent、owner-local AgentFrame history、workflow/
  lineage 与 concrete Agent association。
- [x] LifecycleAgent 使用 `frames` 与 `runtime_binding` JSONB 归属局部事实；全局
  `agent_frames` 与 Product binding table 已删除。
- [x] Dash source 使用单个 canonical document；branch/history/command/effect/change 关系镜像
  已删除。
- [x] Create 前 effect receipt 保持 concrete Agent-owned，可由 `inspect(effect_id)` 查询。
- [x] Runtime、Host、Callback repository/revision schema 与生产组合已删除。
- [x] Product command claim、input queue、background delivery 与 recovery ledger 已删除。

## Command / Read / Stream

- [x] Product input 使用同步 `AgentRunProductInputDeliveryPort` handoff。
- [x] handoff/effect identity 由 Product target + client identity 稳定派生。
- [x] 成功结果始终携带 concrete Agent operation receipt；Agent unavailable直接返回 typed error。
- [x] command retry 使用相同 Agent effect + `inspect` 收敛，不依赖 Runtime operation repository。
- [x] conversation snapshot 直接读取 Complete Agent source并在内存中 normalize。
- [x] production Dash execution callback接入 source-scoped live event sink。
- [x] Complete Agent snapshot、Managed Runtime wrapper与前端feed只保留
  `conversation_history: CanonicalConversationRecord[]`；平行turn/item/active字段已删除。
- [x] committed native history与Core ephemeral delta只在当前进程broadcast；gap/断连通过
  authoritative snapshot恢复。
- [x] live transport只接受`AgentLiveEvent { source, sequence, record }`，前端运行态只由canonical
  `TurnStarted/TurnCompleted`推导。
- [x] Agent terminal failure保留真实 code/message/retryability。
- [x] 用户输入与`TurnStarted`在Agent native history提交后立即进入live，顺序先于首个Core输出。
- [x] Agent实际接纳的surface/initial context写入native history并投影canonical ContextFrame；前端
  直接消费`Platform(ContextFrameChanged)`。

## Product / Workflow

- [x] AgentRun list/workspace在association缺失或Agent read失败时仍返回Product shell。
- [x] command/list不再以Runtime projection currentness、generic revision或surface mirror做gate。
- [x] LifecycleGate waiting items直接进入conversation response。
- [x] Companion、Routine、Workflow与human response统一调用Agent input handoff。
- [x] Companion continuation、Workflow AgentCall与Product protocol saga重复账本已删除。
- [x] channel/gate/routine只在owner-local document保存自身业务事实与下游handoff coordinate。
- [x] 普通Fork继承concrete Agent binding并直接Activate；只有显式Product选型执行
  Frame materialization与Rebind。

## Host / Callback

- [x] Complete Agent Host只保存当前进程attachment、target、binding、generation与callback route。
- [x] Host restart从Product association、当前Agent selection与Agent receipt重新建route。
- [x] callback route/generation/deadline在Host内存fence；真实Tool/Hook owner负责幂等receipt。
- [x] optional Agent program/credential/materialization不可用被隔离为typed unavailable diagnostic。
- [x] Runtime Wire跨进程状态网关从生产组合删除；Remote transport只保留真实placement职责。

## Schema Hard Cut

- [x] 0090–0096删除Runtime/Host/Callback持久化权威与Dash关系镜像。
- [x] 0097把AgentFrame与association收回LifecycleAgent owner document。
- [x] 0098–0103删除Workflow/AgentRun/Companion重复saga、receipt与continuation ledger。
- [x] 0104删除失效的conversation展示设置。
- [x] 0105把Routine/Gate局部receipt字段收敛为input handoff语义。
- [x] 0106把Dash surface从repository并行字段迁入native history，并清理旧字段。
- [x] migration history guard覆盖forward-only迁移历史。
- [x] retired schema readiness/负向搜索不允许旧Runtime/Host/Callback owner重新进入最终schema。

## Specification

- [x] 重写 Runtime kernel、persistence、Host、AgentRun facade、Dash native adapter与conversation
  architecture。
- [x] 更新 database/repository/backend architecture、workflow/capability与frontend/backend
  snapshot/live contract。
- [x] 07-17任务由本任务最终权威模型收口。

## Verification

- [x] 受影响 Rust packages `cargo check`。
- [x] AgentRun conversation、Companion、Gate、Host、Dash与API定向测试。
- [x] frontend contract generation/typecheck。
- [x] migration history guard。
- [x] production源码负向搜索。
- [x] `git diff --check`。

## Final tracer bullet

- [x] 既有 Product binding 在新 Host 进程中按 immutable profile + AgentFrame 恢复 Dash service、
  source route 与 binding generation，首次 authoritative snapshot 读取成功。
- [x] 真实 Composer input 使用 `openai-codex / gpt-5.5 / minimal` 执行成功；Codex adapter 将平台
  最低非零推理级别编码为 Provider 原生 `low`。
- [x] AgentRun `814b65c6-633d-598a-a458-ec98f53a8641` 的真实输入依次渲染
  `mounts_list`、`fs_glob` 两个`dynamicToolCall`与最终 Agent message
  `STREAM_OK Cargo.lock`；页面无未知工具或悬空状态。
- [x] authoritative snapshot 只返回14条ordered canonical records，结构为
  user input → TurnStarted → tool items → agentMessage → TurnCompleted；不返回平行turn/item数组。
- [x] 浏览器重载后从Dash source durable history恢复同一工具卡和最终消息；live partial与durable
  read使用相同presentation identity。
- [x] 首个输出后仍active、仅`TurnCompleted`结束receiving的前端回归测试通过。
- [x] 最终定向测试、contract generation/typecheck、migration guard、源码负向搜索与
  `git diff --check` 全量复核完成后生成 closeout。

## ContextFrame 与输入时序补充验证

- [x] Dash repository序列化结果只保留history中的`SurfaceApplied`，当前surface由history fold恢复。
- [x] `InitialContextInstalled`与surface facts均生成typed `ContextFrameChanged`，authoritative
  snapshot返回现有source的ContextFrame。
- [x] source live订阅首先收到durable用户输入与`TurnStarted`，其后才收到ephemeral Agent delta。
- [x] frontend直接解析canonical `ContextFrameChanged`，用户输入无需等待完整Agent响应才进入feed。
- [x] 0106在空库与当前开发库迁移成功，开发库schema为106，新后端可读取迁移后的source。
- [x] 当前开发环境真实消息验证：Composer在提交时立即清空，canonical用户消息先于Agent输出
  出现；执行中正文持续增长，只有`TurnCompleted`结束运行态，终态Agent message保持展开，
  ContextFrame仍从同一conversation history展示。
- [x] 首个成功Dash回合从accepted user input与最终Agent output生成原生标题，提交
  `ThreadNameChanged`后由snapshot/changes/live投影；实测标题在terminal后更新为
  “编号英文单词列表生成”，主标题与AgentRun列表同步，Product未新增标题表或写路径。

## Draft 首条输入与 materialized context 收口

- [x] ProjectAgent Draft 创建只建立 Product/Agent target，不在创建请求内同步执行首条输入。
- [x] 前端拿到 target 后立即进入 AgentRun 页面，等待 authoritative history/live lane 就绪，再用
  标准 composer command 投递原始首条输入。
- [x] Dash native `SurfaceApplied` 保存带稳定 key/channel 的 materialized instruction 列表；provider
  system prompt 与 ContextFrame 均从该列表派生，不保存或反猜第二份 prompt 事实。
- [x] 0107迁移既有 Dash surface document 到 instruction-list 形态，当前开发库可直接启动读取。
- [x] 定向 Rust/前端测试、contract generation、migration guard、浏览器 Draft timing 与 ContextFrame
  tracer 全部通过。

真实Draft tracer在AgentRun `01652dcc-7884-579d-bf93-332ef33f8f0f`验证：创建提交后175ms进入
目标页面，468ms看到canonical用户消息，首个Agent输出约3.38s、`TurnCompleted`约6.50s；snapshot
前六条为五个materialized instruction ContextFrame与一个tool capability frame，随后才是
`UserInputSubmitted -> TurnStarted -> Agent output`。

## 能力发现、授权与 Surface 增量收口

- [x] Project owner与workflow node在持久化final AgentFrame前，从该frame的canonical VFS统一派生
  Skill baseline、AGENTS.md guidelines与memory inventory；结果写回同一owner-local AgentFrame
  document，不建立Runtime/Product能力投影表。
- [x] Complete Agent Product surface从同一frame编译skill、memory、MCP与workspace/context
  requirements；MCP动态工具仍走native callback，提示词只描述Agent实际接纳的能力。
- [x] Project AgentRun的Task surface同时授予Read/Write；`mounts_list`用List做调用准入，但返回同一
  applied VFS surface的完整operation/path scope。
- [x] Dash Core继续只持有`instructions + tools`；ContextFrame仅由Native Adapter从Dash native
  history反解，不进入Dash领域层或形成第二prompt事实源。
- [x] Native Adapter以相邻`SurfaceApplied` state生成instruction/tool真增量；tool delta支持
  added/removed/changed，authoritative read、changes与live callback共用同一projector。
- [x] 前端按tool变化语义渲染名称、来源、参数数量与description，不默认展开原始JSON schema；
  ContextFrame相同phase/order/time时以稳定frame id收敛顺序。
- [x] 定向验证通过：Rust package check、Product frame discovery、skill/MCP surface、Task/VFS授权、
  Workspace Module final broker tracer、Native surface delta，以及前端22项ContextFrame测试和
  typecheck。
- [x] 重启`pnpm dev`后完成真实Draft tracer：确认Task Write、Canvas create后同一turn可编辑、项目本地
  skills、MCP/memory/mount/workspace module完整ContextFrame、专属工具Card与会话流均来自同一
  authoritative Agent history。

## Surface 语义证据与能力闭环

- [x] `AgentFrame::CapabilityState`作为完整能力事实，Product只编译一份同时包含model prompt与
  structured manifest的surface contribution；Native Adapter只从accepted manifest生成平台
  ContextFrame，不从提示词文本或channel名称反猜能力。
- [x] 工具owner为每个definition声明`ToolProtocolProjector`，并沿Runtime definition、Complete Agent
  surface、Dash accepted surface和native history单向传递；live与durable projection共用该证据。
- [x] 每次ToolCall保存调用时已接受的projector，历史工具消息在surface变更或撤销后仍能稳定恢复
  专属Card，不依赖当前工具清单或工具名约定。
- [x] 同一turn的callback router在单个调用期间固定route，在下一次调用前采用新surface route；
  Canvas create产生的新mount因此能被随后`fs_apply_patch`使用。
- [x] VFS发现将真实NotFound与provider/list/read失败区分；只有完成扫描才能形成权威空skill inventory，
  `.agents/skills`和`skills`均从AgentFrame的canonical VFS派生。
- [x] 0108迁移为既有Complete Agent surface与tool history补齐结构化presentation/projector证据，避免
  新代码读取开发数据库时在JSONB反序列化阶段失败。
- [x] 定向Rust测试、受影响package check、contract generation/check、frontend typecheck、migration
  history guard与`git diff --check`通过。
- [x] 0108在当前开发数据库执行成功，新后端可读取既有source/effect document。
- [x] AgentDash canonical Turn直接承载`AgentDashThreadItem[]`，turn边界不再把Native工具消息反序列化
  为vendor协议；`fsRead`/`fsApplyPatch`等专属消息可同时穿过item事件和turn完成事件。
- [x] ToolCall在副作用执行前、ToolResult在每轮完成后立即进入Dash native history；即使provider达到
  round limit，已执行的完整工具轨迹仍可从authoritative snapshot恢复。
- [x] Canvas VFS provider进入生产provider registry；Canvas mount访问权从Canvas归属与
  LifecycleAgent创建者事实计算，个人owner获得write，共享项目资源保持read-only。
- [x] capability篮子完整投影tool path、MCP、VFS、skill discovery、memory、companion、channel与
  workspace module；空inventory也显式呈现，避免把“0项”误判为能力链缺失。

真实能力闭环在AgentRun `9851f356-75b7-5d67-93db-bc99836dfbbe`验证：开发库迁移到schema 108，
authoritative history重载后恢复15个项目/内置Skill、MCP、`main`/`lifecycle` VFS、Workspace
Modules、0个Memory source、0个Companion Agent和0个Channel。Canvas attach产生新的accepted
CAPABILITY frame，随后`READ -> EDIT -> READ`成功把`cvs-capability-tracer://src/main.tsx`改为
`Capability tracer passed`；三条工具消息均以专属Card恢复，会话标题和terminal状态保持一致。

## 工具定义与结构化结果展示收口

- [x] `DashSurface.tools`成为工具说明的唯一accepted事实：完整description/schema进入provider
  `tools[]`，同一列表按需生成模型可读的参数类型、必填性与嵌套字段摘要。
- [x] Core callback、Dash event/folded state与Native Adapter统一传递typed content parts和structured
  details；provider transcript与AgentDash ThreadItem从这份结果分别投影。
- [x] owner声明的`ToolProtocolProjector`继续固定Read/Edit/Shell等专属Card family；展示参数缺失不会
  在实际工具owner校验之前中断执行。
- [x] `MemoryContext`进入surface presentation协议，使memory instruction从accepted native history
  投影为对应ContextFrame，而不是复用assignment展示类型。
- [x] 0109迁移native history中的tool result与memory presentation；0110继续统一folded item snapshots，
  并解包旧VFS callback envelope。当前开发库升级到schema 110且不存在encoded result envelope。
- [x] Rust定向/包级测试、contract generation/check、frontend typecheck、migration guard与真实浏览器
  tracer通过。
- [ ] 用户在接力设备完成验收后生成最终closeout并归档任务。

真实浏览器回归在既有AgentRun“问候与项目协助”验证：历史ContextFrame和Read Card可从Dash source
恢复；新用户输入先进入canonical feed，随后出现执行中状态与Read工具项。`fs_read`实时卡展示目标
路径、8行正文与行号，最终返回`READ_UI_OK`；整页重载后用户消息、工具项、Agent输出和ContextFrame
保持一致。
