# Research: 前端与跨层稳定边界耦合审计

- Query: 全局审计 frontend packages/features、Zustand/store/hooks/services/generated DTO、stream reducer/effect/renderer，以及 cloud/local/desktop/Tauri/HTTP/NDJSON/deployment 边界；识别重复解释、store 旁路、DTO 漂移、命令式副作用与 durable event 混合、composition root 泄漏、本地/云端语义分叉和闭环测试缺口。
- Scope: internal
- Date: 2026-07-26
- Repository anchor: `main@8dc12f7385070f63fd65b5a98247df95edeeae7b`（直接读取 `.git/HEAD` 与 `.git/refs/heads/main`；未执行 git 命令）

## Findings

### 风险总览

| ID | 严重度 | 失效边界 | 典型触发器 | 爆炸半径 |
| --- | --- | --- | --- | --- |
| F1 | Critical | live transport coordinate → Session imperative effect | terminal authoritative reload、reconnect、ephemeral record 收敛 | Session 副作用漏发/错发，Workspace/Task/title 刷新与自动展示失真 |
| F2 | High | generated Agent live contract → NDJSON runtime validation | canonical record 新 variant、嵌套字段变更、坏数据 | stream 后段异常或静默协议漂移 |
| F3 | High | Workspace presentation port → global Zustand store | iframe 延迟回调、target 切换、新 tab type | 错 workspace 布局持久化、未注册 URI/type 绕过 |
| F4 | High | Project event transport → product owner invalidation | 新事件 variant、新 projection、新 owner | App/store/feature 多处协同修改与重复刷新 |
| F5 | High | Rust HTTP contract → JavaScript JSON wire | 任意 `u64`/`usize` 字段增长或新增 endpoint | generated 类型与运行值不一致，service 局部 mapper 扩散 |
| F6 | High | Rust Tauri command → TS desktop client | command/DTO/state variant 调整 | packaged desktop 才暴露的运行时错误 |
| F7 | High | backend path semantics → shared directory view | Linux/Unix backend 目录浏览 | POSIX 绝对路径变相对路径，选错 workspace |
| F8 | High | Terminal application service → tab renderer | spawn response/错误/owner fence 变化 | UI、HTTP、store、tab、projection 同步修改 |
| F9 | Medium-High | Project StateChanged → Story read model | Story enum/字段或事件 payload 策略变化 | patch/refetch 行为静默改变，前端重复 enum |
| F10 | Medium-High | generated DTO → feature service/view model | Project Agent、file picker、browse route 变化 | 缺字段被默认值吞掉，内部 API 漂移无法 fail-fast |
| F11 | High | release gate → executable cross-layer contract | cloud image/desktop 构建 | 类型检查通过但生成物、IPC、关键闭环未验证 |

### F1 — Session 命令式副作用丢失真实 live sequence，改用可收缩的数组下标

**根因类别：重复 identity / 命令式副作用与 durable read model 混合 / transport coordinate 泄漏。**

1. 真正的 `AgentLiveEvent.sequence` 是 source/process-local 的稳定传输坐标，generated contract 明确说明它只在当前 Complete Agent service process 内有序，并在进程重启后重置（`packages/app-web/src/generated/agent-service-api.ts:5-17`）。
2. feed connection 接收完整 `AgentLiveEvent`，但投影后只发布 `ManagedRuntimeSnapshot`；`sequence` 没有随 record 保留。terminal reload 会用 authoritative snapshot 替换当前 history，再按 `presentation_id` 合并 pending durable/live record（`packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.ts:56-63`、`:84-110`、`:144-163`）。
3. hook 只在第一次 `onBaseline` 建立 `baselinePresentationIds`，之后的 authoritative reload 只调用 `onProjection` 更新 snapshot，不更新 baseline generation 或 live coordinate（`packages/app-web/src/features/agent-run-runtime/model/useManagedRuntimeFeed.ts:34-37`、`:65-78`）。
4. Session adapter 显式把每条 record 的 `runtimeSequence` 写成 `null`，再用 `records.map(... index + 1)` 伪造 `event_seq`（`packages/app-web/src/features/session/model/useSessionStream.ts:44-60`、`:73-95`、`:115-128`）。baseline boundary 同样从当前数组位置计算（`:133-139`）。
5. UI 用这个数组下标作为命令式副作用 cursor，只派发 `event_seq > lastSeenSeq` 的事件（`packages/app-web/src/features/session/ui/SessionChatViewModel.ts:26-47`）；cursor 由组件 ref 跨 snapshot reload 保留（`packages/app-web/src/features/session/ui/SessionChatView.tsx:316-331`）。

**可复现的结构性失败：** live ephemeral records 被 terminal snapshot 收敛后，`conversation_history` 可缩短或重排；旧 cursor 仍是收敛前数组位置。随后新的 live platform/product event 即使拥有更大的真实 `AgentLiveEvent.sequence`，其新数组下标也可能小于等于旧 cursor，从而不再触发 Task、Workspace presentation、title 等副作用。相反，若 baseline 边界因数组变化后退，同一 durable record 也可能重新进入命令式 lane。`presentation_id` 能解决 record 合并 identity，不能替代 transport occurrence coordinate。

**测试证据与缺口：**

- `useSessionStream.test.ts:15-34` 只对人为传入的 baseline set 测 helper；“新 snapshot 后提升 baseline”的测试并没有经过 hook/connection，而实现也不会在后续 reload 更新该 set。
- `SessionChatViewModel.test.ts:22-69` 只测试固定整数序列，没有覆盖 `baseline → ephemeral live → TurnCompleted → authoritative history 缩短 → later live`。
- connection 测试覆盖 terminal snapshot 收敛和 reload 期间保留新 record（`managedRuntimeFeedConnection.test.ts:302-377`），但没有把结果继续送入 Session effect cursor。

**建议目标边界：**

- feed owner 同时输出两个不同概念：authoritative read model snapshot，以及只供命令式副作用消费的 typed live delta lane。
- delta coordinate 至少包含 `{connection_epoch, source, AgentLiveEvent.sequence, presentation_id, baseline_generation}`；副作用 cursor 使用真实 source-local sequence 并由 connection epoch 隔离，绝不使用 projection 数组下标。
- snapshot hydration 只更新 read model；live effect dispatcher 只消费 feed-owned delta，不从 durable/ephemeral 混合数组反推“新发生的事件”。
- 建立贯穿 transport → connection → hook → Session dispatcher 的单个闭环测试，固定 terminal reload 和 reconnect 两类场景。

### F2 — Agent live NDJSON 只有手写浅层 guard，generated contract 不是运行时事实源

**根因类别：名义 generated boundary / runtime validation 缺失。**

`parseLiveEvent` 只校验顶层 object、`source` 字符串、十进制字符串 `sequence`、非空 `presentation_id` 和 `event.type` 字符串，随后直接 `return payload as AgentLiveEvent`（`packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedTransport.ts:26-49`）。它没有校验：

- canonical record durability、envelope source/trace/observedAt；
- event discriminant 对应的 payload；
- U64 上界；
- 新旧 contract variant 的完整闭包。

生成代码已经有 `decodeAgentServiceU64`，可校验 canonical unsigned decimal 与 `u64` 上界（`packages/app-web/src/generated/agent-service-codecs.ts:83-100`），但没有生成/使用 `AgentLiveEvent` 全量 decoder。坏 payload 可以穿过 transport，直到 Session 在 `Date.parse(record.presentation.envelope.observedAt)` 抛错（`useSessionStream.ts:63-69`），或者更晚在 reducer/renderer 中以不相关错误暴露。

现有 transport 测试只覆盖 canonical happy path 和已经删除的 telemetry shape（`managedRuntimeFeedTransport.test.ts:5-50`），没有 malformed nested record、错误 durability、越界 sequence、非法 trace/observedAt、合法 discriminant 配非法 payload。

**建议目标边界：** `agentdash-agent-service-api` generator 同时生成完整 runtime decoder；transport 只拥有 fetch、NDJSON framing、reconnect 和 decoder error reporting。所有内部流 envelope 都由生成 validator fail-fast，新增 variant 的 validator fixture 与 Rust serialization fixture进入 `contracts:check`。

### F3 — Workspace tab 的稳定入口存在，但 extension/canvas 直接旁路到全局 store

**根因类别：cross-feature store bypass / workspace coordinate 丢失 / composition ownership 泄漏。**

store 已明确提供原子边界 `openOrActivateInWorkspace(workspaceKey, typeId, uri, options)`，原因是命令式展示可能先于 WorkspacePanel effect，必须先绑定 workspace 再开 tab（`packages/app-web/src/stores/workspaceTabStore.ts:102-114`、`:292-297`）。没有 `options` 时，`openOrActivate` 不要求 type 已注册，URI 也默认通过（`:53-61`、`:274-290`）。

然而：

- Extension panel 的 module-global bridge service 直接调用 `useWorkspaceTabStore.getState().openOrActivate(typeId, uri)`，既没有 workspace key，也没有 registry options（`packages/app-web/src/features/extension-runtime/ui/ExtensionWebviewPanel.tsx:30-39`）。
- iframe bridge 的 service port 本身只有 `(typeId, uri)`，丢失 workspace coordinate（`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:32-50`）；`workspace.open_tab` 接受 iframe 给出的任意字符串 type/URI（`:76-93`）。
- bridge parser 的 method/type 也是开放字符串（`packages/app-web/src/features/extension-runtime/model/bridge.ts:5-20`、`:22-47`）。
- Canvas tab 内部同样直接调用 raw store `openOrActivate`（`packages/app-web/src/features/workspace-panel/tab-types/canvas-tab.tsx:25-34`）。

延迟 iframe callback 在 AgentRun target 已切换后，可能把旧 extension 意图写入当前 workspace；未注册 type/非法 URI 还能绕过当前 tab registry 并被持久化。现有 bridge 测试只证明 intent 被转发（`features/extension-runtime/model/bridge.test.ts:183-197`），store 测试只验证正确调用稳定入口时的首屏行为（`stores/workspaceTabStore.test.tsx:21-70`），没有测试旧 iframe 回调或 registry 拒绝。

**建议目标边界：** WorkspacePanel/composition owner 注入 workspace-scoped `WorkspacePresentationPort`，固定 `{workspaceKey, registrySnapshot}`；extension bridge 只发布 typed intent，不能 import global store。Canvas/extension/未来 tab producer 共用同一 port。闭环测试覆盖 target 切换后的旧 callback、未注册 type、非法 URI 和持久化结果。

### F4 — Project event 仍是 process-global raw bus，事件解释分散在 App 与多个 feature/store

**根因类别：composition root 业务解释 / owner invalidation 未收口 / 多点 exhaustiveness。**

`eventStore` 在 Zustand 之外维护 module-level listener `Set`，对四类业务 envelope 分支后原样广播（`packages/app-web/src/stores/eventStore.ts:27-42`、`:68-97`）。随后：

- `AppContent` 直接 import Project/Coordinator/Story stores，并解释 backend 与 Story 事件；一个 backend event 同时触发两个 refresh 路径（`packages/app-web/src/App.tsx:268-309`）。
- AgentRun list hook 自己订阅 raw event，再解释 `ControlPlaneProjectionChanged.agent_run_list`（`packages/app-web/src/features/agent/agent-run-list-state-store.ts:341-368`、`:393-398`）。
- AgentRun workspace hook 再次订阅同一 raw bus（`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:331-349`），使用另一套 planner（`controlPlaneModel.ts:73-85`、`:126-135`）。
- Story store 又直接解释 `ProjectStateChange` payload 与 refetch/patch 策略（`packages/app-web/src/stores/storyStore.ts:282-337`）。

这使 “新增一个 project event/projection/owner” 必须同时审查 transport switch、App switch、每个 feature subscription、各 store refresh 去重。composition root 不再只是 wiring，而是产品事件语义 owner。当前只有局部 parser/planner/store tests；没有一个 exhaustiveness/dispatch integration test 证明每个 generated variant 恰好到达正确 owner、不会漏达或重复达。

**建议目标边界：** 建立单一 Project Event Application Dispatcher：generated envelope → exhaustive planner → owner-scoped typed invalidation commands。App 只连接 transport 与 dispatcher；各 store 暴露 `invalidate(ownerCoordinate, reason)` port，不订阅 raw bus。对 generated union 做 `never` exhaustiveness，并用 integration test 断言每个 variant 的目标 owner、去重和 project/target 隔离。

### F5 — 通用 HTTP generator 把 Rust 整数声明为 `bigint`，但 JSON wire 从不产生 bigint

**根因类别：wire type 不诚实 / mapper 漂移被局部“修复”。**

通用 `agentdash-contracts` 生成文件把多个 HTTP `u64`/`usize` 字段直接声明为 TypeScript `bigint`，例如：

- Extension artifact `byte_size`（`packages/app-web/src/generated/extension-package-contracts.ts:6`）；
- Shared Library nested artifact `byte_size`（`packages/app-web/src/generated/shared-library-contracts.ts:25`）；
- Canvas revision/state/event/max bytes 等（`packages/app-web/src/generated/interaction-contracts.ts:8-28`、`:42`、`:70`）。

标准 `response.json()` 只能得到 number/string/null/object，不能得到 bigint。于是 Extension service 接受 `number|string|bigint` 并逐字段重建完整 generated DTO（`packages/app-web/src/services/extensionPackage.ts:43-86`）；Shared Library service 在 typed value 上再次 `BigInt(...)`（`packages/app-web/src/services/sharedLibrary.ts:66-74`）。测试甚至把这种不一致固化为预期：“normalizes ... JSON number to bigint”（`packages/app-web/src/services/sharedLibrary.test.ts:49-67`）。

相比之下，Agent Service 明确把 wire U64 定义为 branded decimal string，并提供范围 decoder（`agent-service-api.ts:14-17`、`agent-service-codecs.ts:83-100`）。因此问题不是 JavaScript 无法正确表达整数，而是两个 generator 对“wire shape”采用了不同语义。

**建议目标边界：**

- generator 必须声明真实 JSON wire：超出安全整数语义的 Rust integer 使用 canonical decimal string/branded wire type；有明确范围上限的字段才使用 number。
- generator 为 endpoint/DTO 生成 decoder/encoder；service 不再选择性 normalize。
- 用 Rust serialization fixture → JS decoder → encoder roundtrip 覆盖每种 integer policy，包括 `Number.MAX_SAFE_INTEGER + 1` 和 `u64::MAX`。
- 这是全局 contract 子任务，不能只修 Extension/Shared Library 两个 mapper。

### F6 — Tauri IPC contract 完全手工镜像，Rust 与 TS 可各自通过检查但运行时漂移

**根因类别：跨语言 contract 缺失 / package public boundary 泄漏。**

`@agentdash/core/local-runtime` 手写了整套 Tauri 视图，包括 Local Runtime state/status、profile、MCP、update、Desktop API 与 client port（代表性位置 `packages/core/src/local-runtime/index.ts:1-65`、`:226-289`）。Rust 实际 DTO 分散在：

- `agentdash-local::runtime`（`crates/agentdash-local/src/runtime.rs:52-88`）；
- desktop profile（`crates/agentdash-local/src/desktop_profile.rs:10-51`）；
- update policy（`crates/agentdash-local-tauri/src/desktop_update.rs:18-49`）；
- Tauri `main.rs` 与 settings/codex modules。

当前已有可观察漂移：TS `LocalRuntimeStatus` 额外声明可选 `registration`，并把 `registration_source` 收窄为两个字符串（`packages/core/src/local-runtime/index.ts:28-45`、`:65`）；Rust status 没有 `registration`，且 `registration_source` 是开放的 `Option<String>`（`crates/agentdash-local/src/runtime.rs:69-88`）。

TS adapter 的所有 `invoke()` 都只靠调用点泛型断言返回值，没有 runtime decoder（`packages/app-tauri/src/runtimeApi.ts:31-93`）。Rust command manifest 在 `main.rs:378-404` 单独维护；没有 command name/argument/result parity check。更明显的是 desktop settings 为复用 Rust generated OAuth DTO，直接相对导入 `../../app-web/src/generated/llm-provider-contracts`（`packages/app-tauri/src/desktopSettings.ts:1-13`），说明 generated contract 被错误地放在应用私有源码目录，而不是共享 contract package。

现有 `desktop_check` 只有 icons、共享 package typecheck、app-tauri typecheck、Rust shell check（`scripts/lib/quality-gates.js:107-114`）。这些检查无法证明 Rust `generate_handler!`、invoke 名称、参数 casing 和返回 JSON 同构。仓库中也未找到 `runtimeApi`/command manifest parity test。

**建议目标边界：** 从 Rust Tauri command DTO/manifest 生成共享 `@agentdash/contracts-desktop`（或同等独立 package），同时生成 typed invoke client 与 runtime decoder。`app-tauri` 只能依赖该 package public API，不可相对导入 app-web 源码。架构测试比较 Rust command manifest 与 TS client method/argument/result，全量 packaged smoke 至少调用 snapshot/settings/browse/update 四条命令。

### F7 — Shared DirectoryBrowser 把所有 backend 路径解释成 Windows 路径，POSIX 绝对根丢失

**根因类别：local/cloud semantic divergence / path contract 信息不足。**

同一共享视图服务桌面本机浏览与 Web 远程 backend 浏览：

- Web wrapper 把 `/backends/{backendId}/browse` 结果直接交给 shared view（`packages/app-web/src/features/workspace/directory-browser-dialog.tsx:20-41`、`packages/app-web/src/services/browseDirectory.ts:3-20`）。
- app-tauri 也把 Tauri `desktop_browse_directory` 返回值声明为同一个 views 类型（`packages/app-tauri/src/runtimeApi.ts:11`、`:91-93`）。
- Cloud API/Relay 契约只给绝对 path 字符串，没有 path flavor（`crates/agentdash-api/src/dto/backend.rs:67-83`；`crates/agentdash-relay/src/protocol/workspace.rs:144-158`）。

shared view 却固定调用 `normalizeWindowsPath`，再把 `\` 替换成 `/` 后 `split('/').filter(Boolean)`，从空字符串开始累积 breadcrumb（`packages/views/src/directory-browser/DirectoryBrowserDialog.tsx:22-49`）。对 `/home/user`，生成的 breadcrumb path 是 `home`、`home/user`，丢失最前面的 `/`；点击 breadcrumb 会向 Linux backend 发送相对路径。load 结果也无条件走 Windows normalization（`:65-75`）。

仓库未找到 DirectoryBrowser 测试。该问题会让 Linux cloud runner/远端 backend 的 workspace 选择产生错误路径，而 Windows desktop 看起来正常，是典型跨端语义分叉。

**建议目标边界：** backend 返回 typed path presentation，例如 `{path, style: "windows"|"posix", root, segments}`，或由 backend adapter 直接返回不可歧义的 breadcrumb segments；shared view 不推断 OS path grammar。测试同一 view 贯穿 Windows drive/UNC 与 POSIX `/`/absolute child，验证每个 breadcrumb roundtrip 回原绝对路径。

### F8 — Terminal tab renderer 同时拥有 HTTP、DTO mapper、错误策略、store 注册和 tab mutation

**根因类别：renderer 穿透 application/transport/store 边界 / 过宽变化耦合。**

`terminal-tab.tsx` 直接 import authenticated fetch、API mapper、runtime route、Terminal store 与 WorkspaceTab store（`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:8-24`）。组件内部：

- onData/onResize 直接发 HTTP，并把所有错误吞掉（`:107-130`）；
- spawn 直接构造 endpoint/body、解析 HTTP error/JSON、手写 response mapper（`:272-300`、`:331-377`）；
- 成功后自己向 Terminal store 注册一个局部状态，再修改 tab URI（`:303-324`）。

后端 spawn 实际返回的不止 `terminal_id/process_id`，还包含 `runtime_thread_id`、`terminal_owner_epoch_id`、`latest_source_sequence`、`max_output_bytes`（`crates/agentdash-api/src/routes/terminals.rs:196-203`）。UI 的手写 `TerminalSpawnResult` 只声明两个字段（`packages/app-web/src/types/terminal.ts:41-60`），renderer 因而丢弃 owner fence/sequence 等 projection identity。后续 Product projection 又单独拥有 terminal owner/sequence，这形成“先由 renderer 猜一个 local state，再由 durable projection 校正”的双 owner。

现有测试覆盖 terminal URI 和 Product projection consumer，但仓库未找到 `terminal-tab` spawn/input/resize 闭环测试。输入失败被吞掉意味着 UI 仍显示 running，直到另一条 projection 才可能纠正。

**建议目标边界：** 建立 Terminal Application Port，拥有 generated spawn/input/resize/kill contract、错误分类和 owner coordinate；renderer 只发送 intent/渲染 projection。spawn 成功由同一 projection owner提交 store/tab presentation，或返回包含 owner fence 的 typed receipt 后由 consumer接管，不能由 renderer 构造第二份 terminal truth。闭环测试覆盖 spawn receipt → projection → tab URI、input failure、target switch 与 terminal owner epoch。

### F9 — Story StateChanged 同时被当成 entity snapshot 和 invalidation，前端重复枚举后端 union

**根因类别：事件语义双重化 / DTO mapper drift。**

generated `ProjectStateChange.payload` 仍是 `Record<string, JsonValue>`（`packages/app-web/src/generated/project-contracts.ts:27-35`）。Story service 为判断 payload 是否“像完整 Story”，重新声明 status/priority/type 三组 generated enum allowlist，并手工验证字段（`packages/app-web/src/services/story.ts:17-68`）。Story store 若 guard 通过就本地 patch，否则按 id refetch（`packages/app-web/src/stores/storyStore.ts:300-313`）；delete 又从同一 generic payload 读取 `project_id`（`:315-324`）。

因此同一个 `story_updated` 事件有两种非显式语义：完整 snapshot 或 invalidation。后端只要新增 Story enum/必需字段，前端 guard 便可能从 patch 静默退化为 refetch；若 payload 是合法 partial update，则行为又依赖“碰巧包含了哪些字段”。generated Story union 已存在（`packages/app-web/src/generated/story-contracts.ts:10-24`），前端重复 allowlist 没有提供稳定边界。

**建议目标边界：** 明确选择一种 contract：

- 默认使用 typed invalidation `{owner, entity_id, reason}`，Story owner 从 read API 获取真值；或
- 对确有低延迟需求的事件定义 generated discriminated snapshot payload，保证完整 `StoryResponse`。

不要让 generic payload 的字段完整度在运行时决定事件语义，也不要在 service 重复 enum。

### F10 — 多个内部 HTTP route 仍使用 unknown/手写 DTO，generated boundary 只是局部成立

**根因类别：内部 API 过渡 DTO 长期化 / 默认值吞错。**

代表性证据：

- Project Agent 已有 generated `ProjectAgentSummary`，但前端又用 `Omit` 改写 executor/preset optionality（`packages/app-web/src/types/project-agent.ts:71-96`）。service 请求 `Record<string, unknown>[]`，缺失字段默认成空字符串或“未命名 Agent”，再强制 cast effective config（`packages/app-web/src/services/project.ts:25-47`、`:144-146`）。后端字段缺失不会 fail-fast，而会进入 UI 成为合法默认值。
- File Picker 前后端共同维护一套 camelCase route-local DTO：Rust 明确 `#[serde(rename_all = "camelCase")]`（`crates/agentdash-api/src/dto/file_picker.rs:9-64`），TS 再手写 `FileEntry/ListFilesResponse/BatchReadFilesResponse`（`packages/app-web/src/services/filePicker.ts:4-55`）。这直接偏离项目 `snake_case + generated wire` 规则，且没有 generator/check 覆盖。
- Browse Directory 的 API DTO、views DTO、Tauri DTO 三处同形手写（`packages/app-web/src/services/browseDirectory.ts:3-12`；`packages/views/src/directory-browser/DirectoryBrowserDialog.tsx:3-12`；`crates/agentdash-local-tauri/src/main.rs:225-240`）。

这些并非必要 view model：它们都位于内部 transport 边界或仅改变 null/optional/default 语义。每次 route 调整必须搜索多个 package，且 mapper 可能把契约破坏转成空值，扩大排障范围。

**建议目标边界：** 所有内部 HTTP/Tauri wire DTO 进入 contract crate 与共享 generated package；feature 只从 typed DTO 显式转换真正的 UI view model。删除 route-local camelCase 例外，统一 snake_case。对 unknown 输入只保留在 iframe/外部 provider/用户输入边界。

### F11 — 构建/发布 gate 没有形成 cross-layer contract 闭环

**根因类别：测试装配边界缺失 / deployment 只验证形状不验证语义。**

质量门禁被拆成互不闭合的几组：

- `contracts_check` 存在，但只在 `full_local` 中出现（`scripts/lib/quality-gates.js:20-23`、`:151-166`）。
- `pr_quick` 只有 migration/test-support/shared/frontend/backend check，没有 generated drift、frontend tests、desktop IPC tests（`:116-124`）。
- cloud image workflow 仅调用 `cloud_image_preflight`，而它只是 `pr_quick`（`:147-149`；`.github/workflows/cloud-image.yml:67-71`）。
- `desktop_check` 只有 typecheck/cargo check，没有 invoke parity、runtime decoder、packaged command smoke（`quality-gates.js:107-114`）。
- `deployment_contract` 验证 compose、dry-run、release metadata 和 image command shape，不验证应用层 generated contract 或 cloud/local semantic parity（`:126-137`；`.github/workflows/deploy-contract.yml:42-43`）。

这些门禁各自有价值，但无法阻止本报告中的 F1/F3/F6/F7：它们都能在两侧独立 typecheck 通过。也无法保证生成文件与 Rust 更新在 cloud image 构建时同步，因为 cloud image gate 本身不包含 `contracts_check`。

**建议目标边界：**

1. PR/cloud image 必须包含 generated drift check。
2. desktop gate 加 Tauri command manifest parity 与最小 packaged invoke smoke。
3. 建立三条关键 cross-layer contract tests：
   - Rust live fixture → NDJSON decoder → feed reload → Session effect；
   - iframe intent → workspace-scoped presentation port → persisted layout；
   - Windows/POSIX browse response → shared view breadcrumb → backend request roundtrip。
4. deployment gate 复用这些可执行 contract tests，而不是仅依赖独立 typecheck。

### 建议整改顺序与可拆分子任务

#### Phase 0：先恢复 occurrence/owner 事实权威

1. **Session live delta coordinate 子任务**：实现 feed-owned typed delta lane 和 connection epoch；移除数组下标副作用 cursor；添加 terminal reload/reconnect 纵向测试。解决 F1。
2. **Workspace presentation port 子任务**：把 extension/canvas/tab producer 收口到 workspace-scoped port；禁止 feature import raw global tab store。解决 F3。
3. **Project event dispatcher 子任务**：单一 exhaustive planner + owner invalidation ports；App 退回 wiring。解决 F4/F9 的事件入口问题。

预期收益：变化首先停在明确 owner/coordinate 内，不再穿透 UI ref、global listener 与多个 store。

#### Phase 1：让 generated contract 覆盖真实 wire

4. **统一 JSON integer policy 子任务**：通用 generator 输出诚实 wire type 与 codec，迁移所有 bigint HTTP DTO。解决 F5。
5. **Agent live runtime decoder 子任务**：生成完整 `AgentLiveEvent` decoder 与 Rust fixtures。解决 F2。
6. **内部 route DTO 收口子任务**：Project Agent/File Picker/Browse/Terminal 进入 generated contract；删除 identity rebuild 与 camelCase 例外。解决 F8/F10。

预期收益：字段/variant 变化在生成与 decoder 层 fail-fast，不再靠 service 局部 mapper“猜”后端。

#### Phase 2：收口 desktop/local/cloud 组合边界

7. **Tauri IPC generated client 子任务**：共享 desktop contract package、command manifest、typed invoke decoder、parity test；移除 app-tauri 对 app-web 私有 generated 源码的相对 import。解决 F6。
8. **跨平台 path contract 子任务**：backend 输出 path style/segments，shared view 不解析 OS path；Windows/UNC/POSIX roundtrip tests。解决 F7。
9. **Terminal application port 子任务**：renderer 与 transport/store/tab 分离，spawn receipt 与 Product terminal projection 共享 owner fence。解决 F8。

预期收益：桌面壳、Web Dashboard、remote backend 各自只解释自己拥有的协议，平台差异在 adapter 收敛。

#### Phase 3：将边界变成发布前自动守护

10. **Cross-layer quality gate 子任务**：把 generated drift 纳入 PR/cloud image，补 Tauri parity/packaged smoke 与三条纵向 contract tests。解决 F11。

### 必要内聚与应拆耦合的区分

- **应保留的内聚：**
  - `managedRuntimeFeedConnection` 在 terminal event 后读取 authoritative snapshot，并按 `presentation_id` 收敛 live overlay；这是 feed owner 内部的正确内聚（`managedRuntimeFeedConnection.ts:84-119`、`:144-163`）。
  - Workspace tab store 同时拥有 layout persistence 与原子 workspace bind/open；这正是它应提供的稳定 operation（`workspaceTabStore.ts:102-114`、`:292-297`）。
  - app-tauri 复用 app-web `App` 入口符合“不复制 Web Dashboard 组件树”的规范（`.trellis/spec/cross-layer/desktop-local-runtime.md:215`）。
- **应拆除的变化耦合：**
  - 从投影数组反推 live occurrence；
  - iframe/renderer 直接修改 global store；
  - composition root/多个 feature 同时解释 raw Project event；
  - generated DTO 只提供编译期类型，service 再逐字段重建真实 wire；
  - Tauri Rust 与 TS command/DTO 分开手工维护；
  - shared view 推断 backend OS path grammar。

## Files Found

- `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedTransport.ts` — Agent live NDJSON transport 与浅层手写 parser。
- `packages/app-web/src/features/agent-run-runtime/model/managedRuntimeFeedConnection.ts` — authoritative snapshot、live overlay、terminal reload 收敛 owner。
- `packages/app-web/src/features/agent-run-runtime/model/useManagedRuntimeFeed.ts` — React feed state 与一次性 baseline id 集合。
- `packages/app-web/src/features/session/model/useSessionStream.ts` — canonical records 到 legacy Session envelope 的数组下标映射。
- `packages/app-web/src/features/session/ui/SessionChatViewModel.ts` — live imperative effect cursor/dispatcher。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx` — cursor ref 生命周期与副作用调用点。
- `packages/app-web/src/stores/workspaceTabStore.ts` — layout persistence 和 workspace-scoped stable operation。
- `packages/app-web/src/features/extension-runtime/{model,ui}` — iframe bridge 与 raw global tab store 旁路。
- `packages/app-web/src/stores/eventStore.ts` — process-global Project event listener bus。
- `packages/app-web/src/App.tsx` — composition root 中的 backend/Story 业务事件解释。
- `packages/app-web/src/features/agent/agent-run-list-state-store.ts` — feature-local raw Project event subscription。
- `packages/app-web/src/features/agent-run-workspace/model/{controlPlaneModel,useAgentRunWorkspaceControlPlane}.ts` — 第二套 Project event planner/subscription。
- `packages/app-web/src/stores/storyStore.ts` 与 `services/story.ts` — generic payload 的 snapshot/refetch 双语义和重复 enum guard。
- `packages/app-web/src/generated/{agent-service-api,agent-service-codecs,extension-package-contracts,shared-library-contracts,interaction-contracts,project-contracts,story-contracts}.ts` — generated wire 的真实声明与 integer policy 对比。
- `packages/app-web/src/services/{extensionPackage,sharedLibrary,project,filePicker,browseDirectory}.ts` — 内部 DTO mapper/normalizer/手写 route contract。
- `packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx` 与 `types/terminal.ts` — renderer 内嵌 transport/store/tab mutation 与缩窄 response。
- `packages/core/src/local-runtime/index.ts` — 手写 Tauri TS port/DTO 与本地 runtime view model。
- `packages/app-tauri/src/{runtimeApi,desktopSettings,App}.tsx` — Tauri invoke adapter、app-web 私有 generated import 与 DashboardHost readiness。
- `crates/agentdash-local/src/{runtime,desktop_profile}.rs` — 本机 runtime/profile authoritative Rust DTO。
- `crates/agentdash-local-tauri/src/{main,desktop_update}.rs` — Tauri command manifest 与 desktop-only DTO。
- `packages/views/src/directory-browser/DirectoryBrowserDialog.tsx` — 共享目录视图中的 Windows-only path 解释。
- `crates/agentdash-api/src/dto/{backend,file_picker,terminal}.rs`、`routes/terminals.rs` — Browse/FilePicker/Terminal HTTP wire 事实与 route-local DTO。
- `crates/agentdash-relay/src/protocol/workspace.rs` — remote browse directory relay payload。
- `scripts/lib/quality-gates.js` 与 `.github/workflows/{pr-quick,heavy-check,cloud-image,deploy-contract}.yml` — 当前检查组合和发布入口。

## Code Patterns

- **Projection-to-effect inversion:** transport event 被折叠进 snapshot，UI 再比较 snapshot 数组位置推断“新事件”（`useSessionStream.ts:115-139` → `SessionChatViewModel.ts:33-47`）。
- **Global store escape hatch:** feature/iframe 直接 `useXStore.getState()`，绕过拥有 target/registry 的稳定 operation（`ExtensionWebviewPanel.tsx:30-33`、`canvas-tab.tsx:31-33`）。
- **Raw event fan-out:** transport 广播 generated envelope，各 consumer 自行做 switch/owner filtering（`eventStore.ts:68-97`、`App.tsx:287-301`、`agent-run-list-state-store.ts:393-398`）。
- **Generated type + handwritten runtime truth:** TS 先把 JSON 声明成 bigint/generated DTO，service 再用 unknown/BigInt/逐字段 mapper 修正（`extensionPackage.ts:43-86`、`sharedLibrary.ts:66-74`）。
- **Cross-app private source import:** app-tauri 通过相对路径读取 app-web 私有 generated 文件（`desktopSettings.ts:9-13`）。
- **Renderer as application service:** React tab 直接持有 fetch、error parser、DTO mapper、store transaction 和 navigation（`terminal-tab.tsx:272-377`）。
- **Platform grammar in shared view:** UI 从裸字符串推断 Windows/POSIX path，而 contract 未携带 path style（`DirectoryBrowserDialog.tsx:22-49`）。

## External References

- 未使用外部网络资料；本次判断只依赖仓库代码、tests、spec、workflow 与本地 git 元数据。
- 仓库声明的相关版本：React `^19.2.0`、Zustand `^5.0.11`、TypeScript `~5.9.3`、Vitest `^4.0.18`、Tauri API `^2.0.0`、Tauri CLI `^2.11.1`。上述发现不依赖特定框架版本行为，而依赖项目自己的 owner/wire/coordinate 语义。

## Related Specs

- `.trellis/spec/frontend/architecture.md:7-10` — 内部 API 使用 generated contracts；Runtime snapshot/live 消费同一 canonical protocol。
- `.trellis/spec/frontend/architecture.md:49-51` — transport 只接收 generated `AgentLiveEvent`，canonical item 决定 renderer。
- `.trellis/spec/frontend/state-management.md:7-18` — feed baseline/live dispatcher 与 workspace-scoped tab operation。
- `.trellis/spec/frontend/state-management.md:25-29` — target isolation、reconnect snapshot、TurnCompleted 收敛。
- `.trellis/spec/frontend/type-safety.md:10-11` — snake_case 与 generated wire 单源。
- `.trellis/spec/frontend/type-safety.md:21-38` — generated runtime validator、允许 mapper 的外部边界、禁止重复 enum/identity rebuild。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md:5-24` — Rust generator 是 HTTP/NDJSON/wire 事实源，TS 不复制同名 DTO。
- `.trellis/spec/cross-layer/backbone-protocol.md:51-64` — durable/live canonical schema、source-local sequence、presentation identity 与 turn boundary。
- `.trellis/spec/cross-layer/backbone-protocol.md:141-146` — Rust/live transport/projection 应覆盖的测试层。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:7-14` — Tauri 薄壳、Rust manager owner、TS port/adapter。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:539-543` — Dashboard API origin 与 desktop runner host/Tauri adapter 的职责。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:601-602` — desktop bridge typecheck 与 auto-connect tests 的现有保证。
- `.trellis/spec/cross-layer/deployment-runtime.md:43-45`、`:99-104` — cloud image、Web HTTP、desktop update contract/deployment authority。
- `.trellis/spec/cross-layer/shared-library-contract.md:34-38`、`:78-83` — HTTP snake_case 与后端 DTO 权威。
- `.trellis/tasks/07-26-module-coupling-stable-boundary-review/prd.md:26-45` — 本次全量审计范围、证据要求与只评估不实施约束。

## Caveats / Not Found

- 本次为静态、只读研究，没有运行 dev server、Vitest、Playwright、Cargo 或 packaged desktop；“测试缺口”基于仓库内测试文件与引用搜索，不等价于证明线上一定发生过对应故障。
- researcher 角色禁止 git 操作；commit anchor 通过直接读取 `.git` 元数据获得。未检查工作树状态，也未触碰并行会话修改，因此行号反映 2026-07-26 观察到的共享工作区。
- 未找到 DirectoryBrowser、`terminal-tab` transport、Tauri invoke manifest parity、Project event 全 union dispatch 的闭环测试；如果测试由仓库外 CI/private harness 提供，本报告无法观察。
- 未核验 GitHub branch protection/required-check 配置，因此 F11 只断言各 workflow/gate 自身的组成，不断言某个 workflow 是否被外部规则设为 required。
- `registration_source` 的 TS/Rust 类型差异证明 IPC 没有共享生成事实源；目前尚未找到第三种实际 producer value，因此不把该差异单独认定为当前用户可见 bug。
- `DEFAULT_LOCAL_RUNTIME_SERVER_URL` 在 `@agentdash/core` 为 `3001`，app-web desktop fallback 为 `17301`；调用点通常显式传入当前 Dashboard origin，现有证据不足以把它定性为实际错误。本报告未将其列为主要 finding，但它进一步说明默认值 authority 分散，后续 Tauri contract 收口时应一并消除。
