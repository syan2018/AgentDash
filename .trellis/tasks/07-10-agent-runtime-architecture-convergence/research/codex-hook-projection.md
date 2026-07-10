# Codex Hook 投影与 Agent Runtime 接入层研究

## 1. 结论摘要

平台 Hook 最适合采用“业务定义集中、触发点随因果边界分布、Driver 负责实现翻译”的结构，而不是整体塞进 Application、Executor 或 Agent Core 的某一层。

- Hook rule、workflow/policy contribution、revisioned `HookPlan` 的编译属于 **Business Agent Surface**。这是业务语义，不应由 Executor 或 vendor adapter 解析。
- Thread/Turn、canonical terminal、managed compaction 等平台拥有的生命周期触发点属于 **Managed Agent Runtime**；这些触发点不要求外部 Agent 自己支持 Hook。
- 平台 Tool Broker 内的 pre/post tool hook 由 **Tool Broker** 精确执行；它只覆盖经 Broker 调用的平台工具，不能冒充对 Agent 私有工具的拦截。
- provider request、Agent 私有工具、Agent 内部 subagent、same-loop stop candidate 等内部触发点，必须由 **Runtime Driver/Agent 内部 callback point** 或 Agent 原生 Hook 系统实现。
- Integration/Executor Driver Host 只负责 profile 求交、binding、materialization 生命周期与 routing，不拥有 Hook 规则和决定。
- Infrastructure 只提供脚本 evaluator、artifact store、进程/IPC、secret、持久化和 outbox 等机制。

Agent 不应该用一个 `supports_hooks: bool` 声明能力。Service offer 声明的是逐触发点、逐 effect 的 `HookProfile`；Thread binding 再固定实际的 `BoundHookPlan`、plan revision、materialization digest 和 driver generation。

Codex 确实可以承载较强的 Hook 集成，但“写一个固定 `.codex` 脚本”只是其中一种 artifact projection 技术：

- Codex 从 project/user/system/session/plugin config layer 发现 Hook；project 层可使用 `.codex/hooks.json` 或 `.codex/config.toml` 内联声明。
- App Server 只有 `hooks/list`，没有 `hooks/register` 或通用 host callback RPC。要新增原生 Hook，必须在 Thread 建立前通过 config/session override、project artifact 或 plugin artifact 注入。
- 直接修改项目已有 `.codex/hooks.json` 会与用户配置产生 ownership 冲突。Codex adapter 更适合生成 **AgentDash 管理、按 digest 不可变的 Codex plugin/capability artifact**，通过 Thread 的 selected capability root 绑定；若明确选择 project artifact，再使用 `.codex/agentdash/...` 命名空间和受控 merge，而不能覆盖用户文件。
- Codex 当前只真正运行 command handler；`prompt`、`agent` handler 和 async command 虽存在配置/协议词汇，但 discovery 会跳过。
- Codex 原生 Hook 能实现同步 block、tool input rewrite、permission allow/deny、additional context 与 Stop continuation。单靠 event observation + steer 只能实现未来行为的 advisory follow-up，不能等价实现 `BeforeTool` veto、permission decision、`BeforeProviderRequest` rewrite、managed compaction cancel 或 same-loop `BeforeStop`。

推荐的交付机制优先级是：

1. 平台拥有的边界使用 `HostLifecycle`/`ToolBroker`，不依赖 Agent Hook 能力；
2. 可协同修改的 Native/Enterprise/Codex Core 使用 `DriverCallback`，获得最清晰的统一语义；
3. 未修改的 Codex 使用 `NativeArtifactProjection` 覆盖其原生 Hook 点；
4. 只有确实不需要决策的事件才声明 `Observed`；
5. `SteerApproximation` 仅作为显式弱语义，不能满足 required blocking Hook。

## 2. 当前 AgentDash Hook 的语义不是单一“脚本回调”

现有平台 Hook 已经同时承担多种能力：

- 生命周期触发：SessionStart、UserPromptSubmit、Before/AfterTool、AfterTurn、BeforeStop、SessionTerminal、Before/AfterSubagentDispatch、CompanionResult、Before/AfterCompact、BeforeProviderRequest；
- 输入与上下文：`HookInjection`、tool input rewrite、snapshot refresh；
- 控制决定：block、approval request、compaction decision；
- workflow 行为：step advance、execution log、pending action、通用 domain effect；
- durable effect：terminal hook effect、auto-resume、mailbox/pending action。

证据集中在：

- `crates/agentdash-spi/src/hooks/mod.rs` 的 `HookEvaluationTrigger`、`HookEvaluationQuery`、`HookResolution`；
- `crates/agentdash-domain/src/workflow/value_objects/hook_rule.rs` 的 workflow trigger/rule；
- `crates/agentdash-application-hooks` 的 rule 编译与 provider；
- `crates/agentdash-application-runtime-session`、`application-agentrun`、`application-ports` 中目前散落的 live runtime、terminal effect 与 projection bridge。

因此不能把“支持 Hook”缩减为“能收到某个 callback”或“能 steer”。必须回答两个问题：

1. Agent/Host 能否在正确的因果时间点停住？
2. 该时间点能接受哪些决定，并由谁保证决定真正生效？

## 3. 目标层级与 ownership

### 3.1 按触发点的因果 owner 分布

| Hook 点 | 精确触发点 owner | Agent 是否必须支持 | 说明 |
| --- | --- | --- | --- |
| Thread/Session start | Managed Runtime | 否 | 平台可在 driver start 前评估；若要求注入 Agent 内部原生 session start context，则再匹配 Driver profile |
| User prompt submit | Managed Runtime | 否 | 平台在 `TurnStart` durable accept/driver dispatch 之间执行可精确 block；Agent 内部追加的 prompt 不在此范围 |
| Before/After platform tool | Tool Broker | 否 | 仅覆盖经 Broker 的 Dynamic Tool/MCP/platform tool |
| Before/After Agent-native tool | Agent/Driver | 是 | Host 只看到完成后 item 时已无法阻止执行 |
| Permission request | 产生 Interaction 的边界 | 视来源而定 | Broker permission 由 Host 拦截；Agent 私有 permission 必须由 Driver callback/native Hook 暂停 |
| Before provider request | Agent loop | 是 | 必须在模型请求发出前 callback；event + steer 不可替代 |
| Before managed compact | Managed Runtime | 否 | 平台拥有 prepare/persist/activate saga 时可精确执行 |
| Before Agent-native auto compact | Agent/Driver | 是 | Codex opaque compact 若不扩展 Core，只能 native Hook 或 observed |
| After managed compact | Managed Runtime | 否 | checkpoint/head activation 后产生 canonical trigger |
| Before canonical terminal | Managed Runtime + Driver terminal candidate handshake | 通常是 | 要求同一 Turn 继续时，Driver 必须先提交 candidate 并等待决定；收到最终 terminal 后再 follow-up 已不是 same-loop |
| Session terminal | Managed Runtime | 否 | canonical terminal 持久化后可精确执行 post-terminal effect |
| Platform-owned subagent | Managed Runtime | 否 | 平台自己创建/停止子 Thread 时可精确执行 |
| Agent-internal subagent | Agent/Driver | 是 | 仅观察 Codex subagent item 无法实现 pre-dispatch veto |
| Companion result/mailbox | Managed Runtime/Application product boundary | 否 | 是平台事件，不应要求外部 Agent 暴露 native Hook |

这意味着 Hook runtime 不是一个新的“大一统中间层”。`HookPlan` 统一，但执行点必须落在拥有该状态转换的模块。

### 3.2 各模块应拥有的内容

#### Business Agent Surface

- 汇总 workflow/project/story/task/run/Capability Pack 的 Hook contribution；
- 编译 canonical `HookPlanRevision`；
- 为每条 rule 生成 required trigger、payload、allowed effects、failure policy；
- 判断 required/optional contribution；
- 不感知 Codex TOML/JSON、脚本路径或 App Server DTO。

#### Managed Agent Runtime

- 将 `HookPlanRevision` 固定到 RuntimeBinding/Thread；
- 编排 Host-owned trigger；
- 持久化 invocation、decision、trace、effect/outbox 与 terminal/compaction saga；
- 对 required Hook 做 admission；
- 验证 invocation 对应 binding generation、plan revision、Turn/Tool/Interaction 坐标；
- 恢复超时或 crash 中断的 actionful Hook。

#### Integration/Executor Driver Host

- Service offer 声明最大 `HookProfile`；
- 将 runtime requirement 与 service/transport/policy profile 求交；
- 调用 Driver materialize/apply/revoke；
- 固定 artifact digest、source identity mapping 与 driver generation；
- 不运行 Rhai 业务规则，不解释 workflow effect。

#### Runtime Adapter / Agent Core

- Adapter 把 canonical hook point/payload/decision 翻译成 vendor/native contract；
- Clean Agent Core 只暴露必要 callback facets，例如 provider/tool/stop candidate，并不拥有 AgentDash workflow rule；
- Codex adapter 负责 native artifact、App Server notification、native ID mapping 与 output translation。

#### Infrastructure

- Hook script evaluator；
- artifact 原子写入、权限、digest、GC；
- local IPC/credential；
- repository/outbox/process timeout；
- 不决定何时 compact、是否 block tool 或是否推进 workflow。

## 4. HookProfile：Agent 如何声明能力

### 4.1 不能使用单一布尔值

同一个 Agent 可能支持：

- `BeforeTool` 同步 block 与 input rewrite；
- `AfterTool` context injection；
- `Stop` continuation；
- 但不支持 `BeforeProviderRequest`、custom compaction summary、arbitrary domain effect 或 hot plan update。

因此建议 contract 采用逐点描述，而不是把 Hook 塞进宽泛的 `LifecycleProfile` boolean：

```rust
struct HookProfile {
    points: BTreeMap<HookPoint, HookPointProfile>,
    plan_update_boundary: HookPlanUpdateBoundary,
}

struct HookPointProfile {
    delivery: HookDelivery,
    timing: HookTiming,
    payload_fidelity: HookPayloadFidelity,
    decision_authority: HookDecisionAuthority,
    effects: BTreeSet<HookEffectKind>,
    failure_modes: BTreeSet<HookFailureMode>,
}

enum HookDelivery {
    HostLifecycle,
    ToolBroker,
    DriverCallback,
    NativeArtifactProjection,
    Observed,
    SteerApproximation,
    Unsupported,
}
```

其中：

- `timing` 至少区分 `BeforeActionBlocking`、`AfterAction`、`TerminalCandidateBlocking`、`AfterTerminal`；
- `decision_authority` 至少区分 `HostEnforced`、`AgentEnforced`、`Advisory`、`None`；
- `effects` 应逐项列出 `Block`、`RewriteInput`、`AllowDenyPermission`、`RequestInteraction`、`InjectContext`、`ContinueSameTurn`、`CancelCompaction`、`OverrideCompactionPolicy`、`EmitDomainEffect` 等；
- `plan_update_boundary` 至少区分 `HotAcked`、`NextTurn`、`ThreadResume`、`Rebind`。

### 4.2 Offer、Binding 与 runtime ack

能力声明需要三阶段：

1. `RuntimeOffer.hook_profile` 声明 Driver 的最大能力，由 behavior tests 支撑；
2. Business Agent 编译 `HookRequirement`，Host 求交后生成 `BoundHookPlan`；
3. Driver 返回 `HookPlanApplied { plan_revision, applied_digest, per_point_status, native_artifact_ids, effective_boundary }`，Managed Runtime 只有收到匹配 ack 才允许 required Hook 的 Thread/Turn 继续。

`hooks/list`、配置文件存在或 Agent 自报均不能替代第 3 步。它们最多是 Codex adapter 生成 applied ack 的证据来源。

## 5. 四种 Agent 接入语义

| 机制 | 语义 | 可实现的强能力 | 主要限制 |
| --- | --- | --- | --- |
| `NativeArtifactProjection` | Adapter 将 canonical HookPlan 编译为目标 Agent 原生 config/script/plugin artifact，由 Agent 在原生点执行 | 如果原生事件与 output decision 等价，可同步 block/rewrite/permission/context/continuation | 受原生词汇与加载边界限制；artifact/trust/脚本安全复杂；跨进程 callback 要处理幂等和超时 |
| `DriverCallback` | Agent/Driver 在正确点暂停，向 Managed Runtime 发 `HookInvocation` 并等待 canonical decision | 最容易获得 exact payload、Host-authoritative decision、统一 trace/recovery；适合可修改的企业 Agent/Core | 需要修改 Agent/Driver；同步回调影响延迟；必须定义 timeout、retry、duplicate、disconnect 与 fail policy |
| `Observed` | Adapter 从 vendor event/notification 得到已发生事实，写 canonical trace/event | 审计、UI、metrics、post-action pending action、后续 workflow | 不能改变已完成 action；通常 payload 不完整；不得声明 block/rewrite/permission authority |
| `SteerApproximation` | 在观察事件或平台 rule 评估后，通过 steer/follow-up 向仍活跃 Turn 注入未来指令 | “接下来修正/补做/停止尝试”的 advisory 行为 | 存在竞态；不可撤销 tool/provider/compact；Turn 可能已 terminal；不是 same-loop decision，不能满足 required gate |

### 5.1 callback + steer 是否足够

只有两种情况下足够：

- Hook 本身是 post-action/advisory，例如“工具完成后提醒补测试”；
- callback 发生在 action 前，Agent 明确暂停且真正执行 Host 返回的 block/rewrite decision。此时强语义来自 **blocking callback**，不是来自 steer。

只有 event callback + steer 时，最多是弱化版：

- 可近似 AfterTool、AfterTurn、CompanionResult 的后续指令；
- 不可近似 BeforeTool veto、BeforeProviderRequest、PermissionRequest、BeforeCompact cancel；
- Stop 后再发 follow-up 会成为新 Turn/continuation operation，不能宣称原 Turn 未结束。

因此 `SteerApproximation` 应出现在 capability/profile 与 UI diagnostics 中，并带 `Advisory` semantic strength。

## 6. Codex 原生 Hook 完整能力

### 6.1 事件词汇

Codex 当前 lifecycle Hook 词汇共十个：

- `PreToolUse`
- `PermissionRequest`
- `PostToolUse`
- `PreCompact`
- `PostCompact`
- `SessionStart`
- `UserPromptSubmit`
- `SubagentStart`
- `SubagentStop`
- `Stop`

证据：

- `references/codex/codex-rs/config/src/hook_config.rs:31-51`
- `references/codex/codex-rs/protocol/src/protocol.rs:1471-1483`
- `references/codex/codex-rs/app-server-protocol/src/protocol/v2/hook.rs:15-18`

SessionStart source 又区分 `startup/resume/clear/compact`，见 `hooks/src/events/session_start.rs:23-37`。

Hook payload 包含的共同/常见坐标包括 session/thread、turn、cwd、transcript path、model、permission mode；tool Hook 还包含 canonical tool name、tool use ID、input/response；subagent Hook 包含 agent ID/type；compact Hook 包含 manual/auto trigger。

### 6.2 handler 类型与当前真实实现

配置/协议枚举包含：

- `command`
- `prompt`
- `agent`

但 discovery 当前：

- 只把 command handler 加入 runnable handlers；
- async command 被跳过；
- prompt/agent handler 被跳过并产生 warning。

证据：`hooks/src/engine/discovery.rs:456-556`。因此 Codex adapter 的 capability 只能按当前选定版本声明 `sync command`，不能因为 schema enum 存在就宣称 prompt/agent/async 可用。

所有匹配 command handler 并发运行，结果为展示按配置顺序重新排序；PreToolUse 的多个 `updatedInput` 使用“最后完成”的结果，permission deny 优先。证据：

- `hooks/src/engine/dispatcher.rs:92-124`
- `hooks/src/events/pre_tool_use.rs:105-161`
- `hooks/src/events/permission_request.rs:121-169`

这与 AgentDash 规则的确定性/优先级语义不必相同。将多条平台 rule 各投影为独立 Codex handler 可能改变决策次序。推荐生成 **每个 HookPoint 一个 bridge handler**，由 AgentDash Hook Runtime 在一次 callback 中按 canonical 规则顺序求值，再返回一个聚合结果。

### 6.3 原生决定语义

| Codex event | 当前有效决定/效果 | 重要限制 |
| --- | --- | --- |
| SessionStart | `continue:false` 停止；stdout/`additionalContext` 注入模型 context | SubagentStart 不尊重 `continue:false`，只做 context injection |
| UserPromptSubmit | block/`continue:false`；additional context | 可精确阻止 prompt 继续 |
| PreToolUse | block/deny；`updatedInput`；additional context | 多 rewrite 以 completion order 取最后；unsupported/invalid output常 fail-open 为 failed hook |
| PermissionRequest | allow/deny，任一 deny 胜出 | `interrupt`、updated input/permissions 为保留字段，当前 fail closed/不支持；不能等价产生 AgentDash 自定义 approval workflow |
| PostToolUse | context/feedback；block/stop 后续正常处理 | 工具已经执行，不能撤销 side effect；schema 中 `updatedMCPToolOutput` 当前没有成为通用 rewrite contract |
| PreCompact | `continue:false` 停止本次 native compact | 不支持 keep-last、reserve、custom summary/prompt 等 AgentDash compaction policy effect |
| PostCompact | `continue:false` 停止后续 loop | compaction 已发生，不能回滚或恢复 platform checkpoint head |
| Stop | `decision:block` 生成 continuation prompt；`continue:false` 停止 | 可实现 Codex same-loop continuation；不是 canonical SessionTerminal post-hook |
| SubagentStart | context injection | 无 pre-dispatch veto |
| SubagentStop | Stop-like completion/continuation语义 | 只针对 Codex 自己的 thread-spawn subagent，不能代表所有 AgentDash subagent |

Core 将 Hook additional context 转成独立 developer/context message并写入 conversation，见 `core/src/hook_runtime.rs:595-615`。PreTool、Permission、PostTool、Stop、Pre/PostCompact 都在实际 inner lifecycle 点同步调用，见同文件 `163-425`。

### 6.4 App Server 暴露面

App Server 提供：

- request `hooks/list`：`app-server-protocol/src/protocol/common.rs:667-670`
- notifications `hook/started`、`hook/completed`：同文件 `1624-1626`
- started/completed payload：thread ID、optional turn ID、`HookRunSummary`
- summary：run ID、event、handler、sync/async、thread/turn scope、source path/source、display order、status、timing、output entries。

`hooks/list` 返回每个 cwd 的 hook metadata、warning、error，并包括 key、matcher、command、timeout、source、enabled、`currentHash` 与 `trustStatus`。README 明确说明：

- 按 cwd 的 effective config 发现；
- disabled Hook 也会返回；
- unmanaged Hook 只有 trusted 才 runnable；
- linked worktree 使用 root checkout 的 `.codex` Hook 声明；
- Hook key 的 event/group/handler suffix 当前是 positional。

证据：`references/codex/codex-rs/app-server/README.md:1719-1788`。

但 App Server 不提供：

- `hooks/register/apply`；
- 通用“Hook 到 Host，等待 Host decision”的 server request；
- Hook input payload notification；
- plan revision/applied digest ack；
- native Hook hot update ack。

`hook/started`/`completed` 是 observability，不是 callback ingress。Completed summary 能看到 status/entry，但不能完整重建 tool input、prompt、transcript context 或 canonical decision。

## 7. Codex 配置、路径与生命周期

### 7.1 可发现来源

Codex discovery 合并：

- managed requirements Hook；
- system/user/project/MDM/enterprise/session flag config layer；
- layer config folder 的 `hooks.json`；
- `config.toml` 的 `[hooks]` 内联事件；
- enabled plugin 的 `hooks/hooks.json` contribution。

Project config folder 对应 `.codex` layer，`hooks.json` 固定为该 folder 下文件；源码见：

- `hooks/src/engine/discovery.rs:63-170`
- `hooks/src/engine/discovery.rs:303-364`
- `config/src/config_layer_source.rs:18-19`

项目 `.codex/hooks.json` 与 `.codex/config.toml` 同时声明 Hook 时会合并并 warning，并不是相互覆盖。

command 不是“Codex 固定脚本路径”：它是任意 shell command，Hook JSON 通过 stdin 写入，进程 cwd 是当前 Turn cwd，stdout 作为 decision，stderr/exit code参与失败/阻断语义，timeout 后 kill。见 `hooks/src/engine/command_runner.rs:17-91`。

因此 AgentDash 可以约定固定 bridge，例如：

```text
<immutable-artifact-root>/<plan-digest>/agentdash-codex-hook-bridge[.exe]
```

再让所有原生 event 指向同一 bridge，但这是 AgentDash materializer 的约定，不是 Codex 已有的 `.codex` 固定脚本规范。

### 7.2 三种注入方案

#### A. 生成 Codex plugin/capability artifact（推荐）

- 生成受 AgentDash 管理的 plugin root、`hooks/hooks.json` 与 bridge；
- artifact 目录按 content digest 不可变；
- ThreadStart 通过 selected capability root 选择；
- 不修改用户/项目 `.codex/hooks.json`；
- plugin source 有 `${PLUGIN_ROOT}`/`${PLUGIN_DATA}` 替换，路径比 linked worktree 下相对 `.codex` command 更稳定。

这是最适合 per-binding `NativeArtifactProjection` 的形态。

#### B. 生成 project `.codex` artifact（技术可行，但 ownership 较差）

- Codex 会读取 root checkout 的 `.codex/hooks.json`；
- 可把 bridge 放入 `.codex/agentdash/hooks/...`，声明写入 `.codex/hooks.json`；
- 必须解析并 merge 用户现有文件，不能覆盖；
- linked worktree 的 Hook 声明来自 root checkout，但 command cwd 仍可能是 linked worktree；单纯使用相对 `.codex/...` 路径可能失效，应使用 immutable absolute path或可解析的 root path；
- 多个 AgentDash Thread/plan 共享同一项目文件，容易造成 revision 串扰，除非使用一个稳定 bridge、运行时按 thread/binding 查 plan。

若采用此方案，`.codex` 只应保存稳定 bridge declaration，具体 HookPlan 不应每次重写为不同 handler 列表。

#### C. ThreadStart `config` session override（只适合受控实验）

`ThreadStartParams.config` 会转为 session/CLI overrides，理论上可注入 `hooks` table。它的问题是：

- session flag Hook 仍属于 unmanaged source；
- 要运行通常还需 trust state或 `bypass_hook_trust`；
- `bypass_hook_trust` 明确是 dangerous override，不应作为正式 adapter 机制；
- App Server contract没有专用 typed Hook config字段，错误更晚暴露。

因此不建议作为目标正式路径。

### 7.3 加载与更新边界

Hook engine 从 Thread config snapshot 构造。Codex 支持在 refresh runtime config 时重建 Hook，但现有 refresh 明确主要替换 user layer并保留 session layer，见 `core/src/session/mod.rs:1564-1612`；App Server `config/batchWrite` 的示例也主要管理 user `hooks.state`。

对 project/plugin/session materialization，不应假设写文件后当前 Thread 自动热生效。目标 profile 默认声明：

- Codex artifact Hook：`ThreadStart`/`ThreadResume` 或显式 Driver reload boundary 生效；
- Driver 必须 `hooks/list` 校验 discovered metadata/trust，再进行一条最小 behavior probe或版本 conformance；
- plan revision变化时，如果没有实现并验证专用 reload/ack，就要求 resume/rebind，而不是返回假成功。

## 8. AgentDash Trigger 到 Codex 的映射

| AgentDash Hook | Codex 原生点 | 推荐交付 | Fidelity |
| --- | --- | --- | --- |
| SessionStart | SessionStart | HostLifecycle优先；需要Codex原生context时NativeArtifact | Exact/Native exact |
| UserPromptSubmit | UserPromptSubmit | HostLifecycle可在dispatch前精确执行；原生点可补Agent-native context | Exact，避免双重执行 |
| BeforeTool | PreToolUse | Broker tool由ToolBroker；Codex-native tool由NativeArtifact或DriverCallback | 按tool ownership精确 |
| AfterTool | PostToolUse | 同上 | Post-action exact；不能撤销side effect |
| BeforeProviderRequest | 无 | 修改Codex Core加DriverCallback | Unsupported；event+steer仅advisory |
| BeforeCompact | PreCompact | Managed compact走HostLifecycle；Codex native compact可cancel但无policy override | Partial native |
| AfterCompact | PostCompact | Managed compact走HostLifecycle；native compact仅Observed/有限post decision | Partial |
| AfterTurn | 无独立等价点 | Managed Runtime在canonical terminal前/后按需求定义；不可笼统映射Stop | Host exact或Observed |
| BeforeStop | Stop | NativeArtifact可获得same-loop continuation；长期Driver terminal candidate callback更统一 | Native exact for continuation |
| SessionTerminal | 无 | Managed Runtime在terminal commit后执行 | Host exact |
| BeforeSubagentDispatch | SubagentStart不支持block | 平台subagent走Host；Codex内部subagent需Core callback | Native context-only，gate unsupported |
| AfterSubagentDispatch | SubagentStop近似 | 平台subagent走Host；Codex内部可Observed/Native continuation | 部分映射 |
| CompanionResult | 无 | HostLifecycle/mailbox；需要Agent反应时显式steer/follow-up | Host exact + optional advisory delivery |

最重要的规则是避免同一 Hook 被 Host 与 Codex native point 重复执行。`BoundHookPlan` 必须为每条 rule固定唯一 execution route；Codex notification只用于证明确实发生和关联 trace，不再触发同一 rule。

## 9. Codex NativeArtifactProjection 的推荐设计

### 9.1 单 bridge，而不是每条 rule 一个脚本

生成一个 per-plan/per-binding bridge handler，所有 event input都转发到本机 AgentDash Hook callback endpoint：

```text
Codex native point
  -> immutable bridge executable
  -> normalize source payload
  -> HookInvocation(plan revision + canonical coordinates)
  -> Managed Runtime durable accept/evaluate
  -> canonical HookDecision
  -> bridge translate to Codex stdout/exit contract
  -> Codex applies decision
  -> hook/completed notification reconciles native run
```

这样可保留：

- AgentDash 自己的 rule order；
- 单次 invocation 的 durable trace；
- 一次性 effect/outbox；
- 统一 timeout/failure policy；
- 不受 Codex 多 handler并行与“最后完成 rewrite”语义影响。

### 9.2 callback contract

`HookInvocation` 至少包含：

- canonical HookInvocationId；
- HookPoint；
- RuntimeBindingId/driver generation；
- ThreadId/TurnId；
- native session/turn/tool/subagent coordinates；
- HookPlanRevision/digest；
- normalized payload + fidelity marker；
- deadline、attempt、source adapter/version。

`HookDecision` 至少包含：

- invocation ID/plan revision；
- effect集合；
- translated decision所需数据；
- durable effect receipt；
- semantic strength；
- diagnostics；
- decision digest。

重复 callback 必须返回同一 terminal decision；deadline 后采用 HookPlan 显式定义的 fail-open/fail-closed/abort policy，不能由 wrapper自行猜测。

## 10. Identity、digest、trust 与安全

### 10.1 不要使用 Codex positional key 作为平台 Hook ID

Codex key 形如：

```text
<source identity>:<event>:<group index>:<handler index>
```

源码自己带 TODO，说明 positional suffix 应被 durable hook ID 替代，见 `hooks/src/engine/discovery.rs:502-504`。平台必须保留自己的稳定 `HookRuleId`，Codex key只作为 `NativeArtifactId`/source mapping。

### 10.2 Codex currentHash 不覆盖脚本文件内容

Codex `currentHash` 由 normalized event、matcher 和 command handler config计算，见 `hooks/src/engine/discovery.rs:561-587`。如果 command 字符串仍是同一路径，而该脚本文件内容被替换，Codex trust hash不会随之变化。

这是直接写固定 `.codex/hook.py` 的关键安全缺口。AgentDash materializer 必须：

- 对 bridge binary/script、hooks manifest、event mapping、schema version、adapter version计算自己的 `ArtifactDigest`；
- 使用 digest 命名的不可变目录/文件，command 字符串包含 digest path；
- 原子写入后设为只读，并在 apply 前重新读取校验；
- 将 `PlanDigest` 与 `ArtifactDigest` 分开，二者共同进入 RuntimeBinding；
- `hooks/list.currentHash/trustStatus` 只作为 Codex-native trust证据，不作为平台完整 supply-chain digest。

### 10.3 Trust

- unmanaged Hook 只有 `Trusted` 才 runnable；changed definition变为 `Modified`；managed source为 `Managed`。见 `hooks/src/engine/discovery.rs:589-610`。
- 禁止正式路径使用 `bypass_hook_trust`。
- AgentDash 自动生成 artifact仍需由 service/binding policy显式授权；“平台生成”不等于自动可信。
- 如果采用 plugin artifact，用户/管理员授权 Integration/service instance后，可以由 Adapter管理对应 native trust state；该授权必须绑定 artifact digest和scope。
- `allow_managed_hooks_only` 会忽略 user/project/session/plugin unmanaged Hook。Service offer/profile必须反映此 policy交集，不能在 bind 后才发现 required Hook未运行。

### 10.4 进程与数据安全

Codex command Hook 由本机 shell直接启动、cwd为Agent cwd，并接收 prompt、tool input/output、transcript path等高敏数据。Adapter需要：

- 使用固定 bridge executable，避免拼接用户输入进 command；
- command中不嵌 bearer token，因为 `hooks/list` 会回显 command；
- 使用本机 named pipe/Unix socket、受权限保护的 capability file或进程继承 handle进行认证；
- token/capability绑定 service instance、binding、plan digest、expiry；
- wrapper只转发 HookPoint要求的最小字段；
- stdout只输出一个严格 Codex decision JSON，日志走受控诊断通道；
- 对 child process、timeout、output大小和并发设置上限；
- Hook side effect 不直接在 wrapper中无日志执行，应通过 Managed Runtime durable effect/outbox。

### 10.5 Invocation identity

Codex run ID主要由 event、display order、source path构成；tool Hook会追加 tool use ID，但其他 lifecycle Hook不天然提供全局唯一 invocation。平台 invocation ID应按 canonical coordinate生成/持久化，例如：

```text
binding + generation + thread + turn + hook point
+ tool/subagent/compact operation id + hook rule/aggregate id + attempt
```

App Server `hook/started/completed` 要映射回该 invocation并做 reconcile，但不能成为唯一事实源。

## 11. 对目标 HookProfile 的 Codex 建议声明

未修改 Codex Core、采用 native artifact bridge时，建议保守声明：

| HookPoint | delivery | authority | effects |
| --- | --- | --- | --- |
| SessionStart | NativeArtifactProjection | AgentEnforced | Stop、InjectContext |
| UserPromptSubmit | HostLifecycle 或 NativeArtifactProjection（二选一） | Host/AgentEnforced | Block、InjectContext |
| NativeBeforeTool | NativeArtifactProjection | AgentEnforced | Block、RewriteInput、InjectContext |
| NativePermissionRequest | NativeArtifactProjection | AgentEnforced | Allow/Deny |
| NativeAfterTool | NativeArtifactProjection | AgentEnforced post-action | InjectContext、Feedback/StopFutureProcessing |
| NativeBeforeCompact | NativeArtifactProjection | AgentEnforced | CancelNativeCompact |
| NativeAfterCompact | NativeArtifactProjection/Observed | AgentEnforced post-action | StopFutureProcessing/Trace |
| StopCandidate | NativeArtifactProjection | AgentEnforced | ContinueSameTurn、Stop |
| SubagentStart | NativeArtifactProjection | AgentEnforced context-only | InjectContext |
| SubagentStop | NativeArtifactProjection/Observed | AgentEnforced partial | Continue/Trace |
| BeforeProviderRequest | Unsupported | None | 无 |
| SessionTerminal | HostLifecycle | HostEnforced | platform durable effect |

如果允许修改 Codex Core/App Server，推荐逐步把 actionful inner points提升为 `DriverCallback`：

- callback payload直接带 canonical/source IDs与plan revision；
- App Server提供 Host decision request，而不是 shell wrapper；
- Context/compaction/terminal candidate可与 AgentDash managed runtime对齐；
- native artifact仍可留给用户自定义 local Hook，但不再承担平台关键规则。

这符合“边界不必做得过硬、企业 AgentCore可调整”的方向：先用 profile表达真实能力，后续同一 HookPoint可从 `NativeArtifactProjection` 演进为 `DriverCallback`，而不改变 Business HookPlan。

## 12. 设计验收建议

后续实现前至少建立这些 behavior tests：

1. required `BeforeTool.Block` 在 Codex native tool执行前生效，且 tool side effect未发生；
2. `RewriteInput` 实际改变Codex执行的参数，而不是只改变trace；
3. Hook callback timeout分别按rule failure policy收敛，重试不重复执行domain effect；
4. plan revision改变但未reload/ack时，新Turn admission失败或明确要求resume/rebind；
5. artifact script同路径内容变化能被AgentDash digest发现，即使Codex currentHash未变化；
6. linked worktree下bridge路径仍可执行，且project hook source identity来自root checkout；
7. `hook/started/completed` 能与canonical invocation reconcile，late/duplicate notification不重复推进状态；
8. `Observed`/`SteerApproximation` profile不会通过required `Block/Rewrite/Permission` requirement；
9. platform Tool Broker Hook与Codex native tool Hook不会重复执行同一rule；
10. `SessionTerminal` Hook在canonical terminal持久化后执行，不被错误映射为Codex Stop；
11. disabled/untrusted/modified native Hook使`HookPlanApplied`失败，而不是Thread带着缺失required Hook运行；
12. `allow_managed_hooks_only` 等policy会参与offer/profile intersection。

## 13. 对父架构文档的直接建议

- 在 `LifecycleProfile` 中将 Hook 拆为独立 `HookProfile`，Lifecycle只引用它的digest/summary。
- `HookPlan` 编译留在 Business Agent Surface；durable orchestration进入 Managed Runtime；Driver Host只做求交与apply。
- 工作包 01 定义逐点profile、delivery、timing、effect与semantic strength。
- 工作包 02 持久化 invocation/decision/effect，并实现 Host-owned trigger与terminal candidate handshake。
- 工作包 03 编译 HookPlan，区分 HostLifecycle、ToolBroker、inner Agent Hook和mailbox。
- Codex adapter工作包优先实现 generated plugin/capability artifact + single bridge + applied digest；直接写project `.codex/hooks.json`只在明确选择该ownership模型时采用。
- Codex Core可协同修改时，DriverCallback是长期更干净的actionful Hook路径；不需要把现阶段artifact实现固化成永久协议边界。
- `Observed`和`SteerApproximation`必须保留明确命名，禁止提升为“支持Hook”的布尔结论。
