# Agent Runtime 持久化职责与事实边界清理实施结果

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
