# Research: frontend-terminal-display-navigation

- Query: 前端终端消息展示、历史回放、跳转到终端 tab、命令卡片 promotion 链路。
- Scope: internal
- Date: 2026-07-02

## Findings

### 结论先行

1. live stream 的 `terminal_output` 可以写入 `useTerminalStore.outputBuffers`，但 history hydrate 不会写入 terminal store。`useSessionStream` hydrate 阶段只调用 `reduceStreamState`，而 reducer 又显式把 `terminal_output` / `terminal_state_changed` 从聊天 entries 中丢弃；因此刷新页面、打开历史 session、从保存的 terminal tab 恢复时，终端输出会看起来“没有消息”。
2. `terminal_state_changed` 只更新已注册的 terminal。live/history 中如果先收到状态事件、或者历史只有 terminal event 而没有前端注册过 terminal，`updateTerminalState` 会静默无效；terminal tab 仍可能显示 “运行中”，但 store 没有真实状态对象。
3. 命令执行卡片的“在终端中查看”当前是静态输出 promotion，不是真实可交互终端。它创建 `promote-${item.id}` 这种 synthetic terminal id，把当前 `renderedOutput` 拷贝进 terminal output buffer，再打开 `terminal://promote-${item.id}`；后续真实进程输入仍会走 `/terminals/{id}/input`，对 synthetic id 没有真实后端进程，容易误导用户。
4. “跳转到具体终端不跳转/跳错”的主要 UI 断点在命令卡片直接写 `useWorkspaceTabStore`，没有走 `AgentRunWorkspacePage.expandWorkspacePanel` / `WorkspacePanel.openTab`。右侧 panel 折叠时只会更新 store，不会展开；WorkspacePanel session 初始化晚于点击时，还可能重置 tab store，丢掉刚打开的 tab。
5. 现存前端代码多处调用 `/sessions/*`，包括 session history、session stream、spawn terminal、trace/runtime-control 等。按本任务约束，修复建议不依赖 `/sessions/*` 对外端点；这些调用应作为后续替换缺陷登记，而不是新的前端方案基础。

### 证据

#### Files found

- `packages/app-web/src/features/session/model/useSessionStream.ts` - session history hydrate + live NDJSON hook；live 终端事件在这里被拦截。
- `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts` - `terminal_output` / `terminal_state_changed` 到 terminal store 的派发器。
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts` - session reducer；history hydrate 使用它重建 feed/raw events，并过滤 terminal entries。
- `packages/app-web/src/features/session/model/useTerminalStore.ts` - Zustand terminal store；保存 terminal registry、bounded output buffer、base offset。
- `packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx` - xterm 终端 tab；从 terminal store 增量回放 output，并向真实 terminal input/resize/spawn 端点发命令。
- `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx` - 右侧 workspace panel；暴露 `openTab` 命令式 API，按 active tab 渲染 tab content。
- `packages/app-web/src/stores/workspaceTabStore.ts` - workspace tab 全局 store；负责 open/activate、layout hydrate、layout persist。
- `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx` - 命令执行卡片；实现“在终端中查看” promotion。
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` - AgentRun workspace 页面；持有 `workspacePanelRef` / `rightPanelRef`，有展开并打开 panel 的正确页面级能力。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts` - workspace module presentation 的 control-plane side-effect 模式，可作为 terminal open side-effect 的形态参考。
- `packages/app-web/src/generated/backbone-protocol.ts` - generated `PlatformEvent`，定义 `terminal_output` / `terminal_state_changed` wire shape。
- `.trellis/spec/frontend/hook-guidelines.md` - 规定 history hydrate 与 live side effect 边界。
- `.trellis/spec/frontend/state-management.md` - 规定 store 分层与 AgentRun workspace command/projection 事实来源。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - 规定前端消费 generated contracts，不在前端重定义 wire DTO。
- `.trellis/spec/backend/session/streaming-protocol.md` - NDJSON session stream contract，记录现有 session stream endpoint 形态。

#### Live stream flow

1. Wire type: generated `PlatformEvent` 包含 `{ kind: "terminal_output", data: { terminal_id, data } }` 和 `{ kind: "terminal_state_changed", data: { terminal_id, state, exit_code, message } }`，见 `packages/app-web/src/generated/backbone-protocol.ts:271`。
2. Live NDJSON transport: `createSessionStreamTransport` 默认构造 `/api/sessions/{id}/stream/ndjson`，通过 `x-stream-since-id` 拉取增量，解析后调用 `onEvent(event)`，见 `packages/app-web/src/features/session/model/streamTransport.ts:30`、`packages/app-web/src/features/session/model/streamTransport.ts:82`、`packages/app-web/src/features/session/model/streamTransport.ts:89`、`packages/app-web/src/features/session/model/streamTransport.ts:171`。这是现存 `/sessions/*` 使用，需作为待替换缺陷记录。
3. `useSessionStream` live 回调在 `onEvent` 中进入 `enqueueEventRef.current(event)`，见 `packages/app-web/src/features/session/model/useSessionStream.ts:228`、`packages/app-web/src/features/session/model/useSessionStream.ts:232`。
4. `enqueueEvent` 先调用 `dispatchSessionPlatformEvent`，若派发器返回 true 就直接返回，不进入 React reducer state，注释说明这是为了避免 StrictMode reducer 双重执行导致输出重复，见 `packages/app-web/src/features/session/model/useSessionStream.ts:132`、`packages/app-web/src/features/session/model/useSessionStream.ts:133`、`packages/app-web/src/features/session/model/useSessionStream.ts:135`。
5. `dispatchSessionPlatformEvent` 对 `terminal_output` 调用 `useTerminalStore.getState().appendOutput(terminal_id, data)`；对 `terminal_state_changed` 先校验状态字符串，再调用 `updateTerminalState(terminal_id, state, exit_code)`，见 `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:14`、`packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:19`、`packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:22`、`packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:26`、`packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:33`。
6. `useTerminalStore.appendOutput` 把输出追加到 `outputBuffers`，并维护 `outputBufferBaseOffsets`；buffer 上限是 256 KiB，见 `packages/app-web/src/features/session/model/useTerminalStore.ts:4`、`packages/app-web/src/features/session/model/useTerminalStore.ts:64`、`packages/app-web/src/features/session/model/useTerminalStore.ts:66`、`packages/app-web/src/features/session/model/useTerminalStore.ts:71`、`packages/app-web/src/features/session/model/useTerminalStore.ts:80`。
7. `TerminalView` 订阅 `getOutput(activeId)` 和 `getOutputBaseOffset(activeId)`，用 `lastWrittenOffsetRef` 只向 xterm 写 pending slice，见 `packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:56`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:68`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:133`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:137`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:143`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:146`。
8. live terminal event 不进入 `rawEvents`。因此 `SessionChatView` 的 `collectAllPlatformEvents` live side effect 不会重复处理 terminal output，见 `packages/app-web/src/features/session/ui/SessionChatView.tsx:387`、`packages/app-web/src/features/session/ui/SessionChatView.tsx:391`。

#### History hydrate flow

1. `useSessionStream` 明确先 hydrate 历史，再连接增量流，见 `packages/app-web/src/features/session/model/useSessionStream.ts:1`、`packages/app-web/src/features/session/model/useSessionStream.ts:4`。
2. hydrate 阶段分页调用 `fetchSessionEvents(sessionId, afterSeq, HISTORY_PAGE_SIZE)`，并只执行 `nextState = reduceStreamState(nextState, page.events)`，见 `packages/app-web/src/features/session/model/useSessionStream.ts:206`、`packages/app-web/src/features/session/model/useSessionStream.ts:212`、`packages/app-web/src/features/session/model/useSessionStream.ts:213`。
3. `fetchSessionEvents` 当前调用 `/sessions/{id}/events?after_seq=...`，见 `packages/app-web/src/services/session.ts:54`、`packages/app-web/src/services/session.ts:62`。这是现存 `/sessions/*` 使用，需替换。
4. `reduceStreamState` 会把 durable event 放入 `rawEvents`，再调用 `applyEventToEntries`，见 `packages/app-web/src/features/session/model/sessionStreamReducer.ts:634`、`packages/app-web/src/features/session/model/sessionStreamReducer.ts:666`、`packages/app-web/src/features/session/model/sessionStreamReducer.ts:668`。
5. `applyEventToEntries` 对 `platform.kind === "terminal_output" || "terminal_state_changed"` 直接 `return prev`，见 `packages/app-web/src/features/session/model/sessionStreamReducer.ts:582`、`packages/app-web/src/features/session/model/sessionStreamReducer.ts:589`、`packages/app-web/src/features/session/model/sessionStreamReducer.ts:590`。
6. hydrate 阶段没有调用 `dispatchSessionPlatformEvent`，所以 terminal output 既不出现在聊天 feed，也不写入 terminal store。历史打开时 terminal tab 的 `getOutput(activeId)` 为空，`TerminalView` 的 output effect 直接 return，见 `packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:134`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:140`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:141`。
7. `.trellis/spec/frontend/hook-guidelines.md` 规定 history hydrate 用于重建本地展示状态，而控制面副作用只消费 `historyReplayBoundarySeq` 之后的新 durable event。终端输出回放不是外部控制面副作用，而是 terminal display state hydrate；因此应由明确的 terminal hydrate 通道处理，而不是让聊天 side-effect 处理历史事件。

#### Terminal store and terminal tab flow

1. Terminal registry 是 `session_id -> terminal_id -> TerminalInfo`，output buffer 是 `terminal_id -> string`，见 `packages/app-web/src/features/session/model/useTerminalStore.ts:6`、`packages/app-web/src/features/session/model/useTerminalStore.ts:8`、`packages/app-web/src/features/session/model/useTerminalStore.ts:10`。
2. `registerTerminal` 只在显式注册时写 registry，见 `packages/app-web/src/features/session/model/useTerminalStore.ts:32`、`packages/app-web/src/features/session/model/useTerminalStore.ts:36`。
3. `updateTerminalState` 遍历已有 sessionMap，只有找到 terminal id 才更新；找不到时不创建，也不报错，见 `packages/app-web/src/features/session/model/useTerminalStore.ts:41`、`packages/app-web/src/features/session/model/useTerminalStore.ts:44`、`packages/app-web/src/features/session/model/useTerminalStore.ts:45`、`packages/app-web/src/features/session/model/useTerminalStore.ts:60`。
4. `TerminalView` 对既有 `terminal://id` 只设置 UI status 为 running，实际 terminal state 仍来自 store；如果 store 没有注册 terminal，状态栏会缺少真实 exit code / lost 状态，见 `packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:117`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:119`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:152`。
5. 真正 spawn 新终端时，`TerminalView` 调 `/sessions/{sessionId}/terminals`，成功后注册 terminal，并把 tab uri 从 `terminal://new` 改为 `terminal://{realId}`，见 `packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:223`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:242`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:252`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:253`。这是现存 `/sessions/*` 使用，需替换。
6. xterm 输入和 resize 对真实 terminal id 调 `/terminals/{id}/input` / `/terminals/{id}/resize`，见 `packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:92`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:95`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:102`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:105`。

#### Workspace panel and jump flow

1. `WorkspacePanel` 初始化时如果 `storeSessionId !== sessionId`，会 `initialize(sessionId, null, tabLayoutOptions)`，见 `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:76`、`packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:78`、`packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:79`。
2. `WorkspacePanel` 暴露 `openTab(typeId, uri, options)`，内部使用 `openOrActivate(typeId, uri, tabLayoutOptions)` 并可 refresh content，见 `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:120`、`packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:122`、`packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:126`、`packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:134`。
3. `WorkspacePanel` 根据 `activeTabId` 找 active tab，再通过 registry descriptor 渲染 tab content，terminal descriptor 会渲染 `TerminalView`，见 `packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:194`、`packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:219`、`packages/app-web/src/features/workspace-panel/WorkspacePanel.tsx:230`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:318`、`packages/app-web/src/features/workspace-panel/tab-types/terminal-tab.tsx:325`。
4. `workspaceTabStore.openOrActivate` 只按 `typeId + uri` 找同一 tab，找到则激活，找不到则 `addTab`，见 `packages/app-web/src/stores/workspaceTabStore.ts:252`、`packages/app-web/src/stores/workspaceTabStore.ts:260`、`packages/app-web/src/stores/workspaceTabStore.ts:264`、`packages/app-web/src/stores/workspaceTabStore.ts:267`。
5. `AgentRunWorkspacePage` 有页面级 `expandWorkspacePanel`：先通过 `workspacePanelRef.current?.openTab(typeId, uri, options)` 打开 tab，再 `rightPanelRef.current?.expand()`，见 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:97`、`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:100`、`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:106`、`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:108`。
6. 这个页面级能力已被 control-plane 使用：`openWorkspacePanel` 传给 `useAgentRunWorkspaceControlPlane`，后者在 effect plan 中调用，见 `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:427`、`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:328`、`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:347`、`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:348`。
7. `CommandExecutionCardBody` 没有拿到这个页面级 open capability；它直接 `useWorkspaceTabStore.getState().openOrActivate("terminal", uri)`，不会展开右侧 panel，也不会经过 `WorkspacePanel` 的 `tabLayoutOptions`，见 `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:9`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:52`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:54`。

#### Command card promotion flow

1. promotion id 固定为 `promote-${item.id}`，见 `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:36`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:37`。
2. 若 store 里还没有该 id 输出且当前 `renderedOutput` 存在，就把当前文本拷贝到 terminal output buffer，见 `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:38`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:39`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:40`。
3. 若有 session id，则注册一个 terminal info；`cwd` 来自 item 或 `"platform://"`，`state` 按 card 是否 running 映射为 `"running"` 或 `"exited"`，见 `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:42`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:43`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:46`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:47`。
4. 然后打开 `terminal://promote-${item.id}`，见 `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:52`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:54`。
5. 按钮文案是“在终端中查看”，见 `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:113`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:118`。结合 `TerminalView` 对任何非 `new` id 都启用 input/resize POST 的行为，这个 synthetic tab 会呈现为真实 terminal，但没有真实后端 process。
6. promotion 只在点击时写一次 output。若命令仍在 `inProgress`，后续 `renderedOutput` 增量不会同步到 `promote-*` buffer；再次点击时因为 `store.getOutput(promoteId)` 已存在，也不会追加更新，见 `packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:31`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:32`、`packages/app-web/src/features/session/ui/bodies/CommandExecutionCardBody.tsx:39`。

#### Tests currently present

- `sessionPlatformEventDispatcher.test.ts` 覆盖 live dispatcher：`terminal_output` 写 capped store，`terminal_state_changed` 更新已注册 terminal，见 `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.test.ts:60`、`packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.test.ts:75`。
- `useTerminalStore.test.ts` 覆盖 bounded output、base offset 和 remove cleanup，见 `packages/app-web/src/features/session/model/useTerminalStore.test.ts:20`、`packages/app-web/src/features/session/model/useTerminalStore.test.ts:30`、`packages/app-web/src/features/session/model/useTerminalStore.test.ts:44`、`packages/app-web/src/features/session/model/useTerminalStore.test.ts:55`。
- 未看到覆盖 `useSessionStream` history hydrate 将 terminal event 写入 terminal store 的测试。
- 未看到覆盖命令卡片 promotion 打开 panel、区分 synthetic/read-only output tab 与真实 terminal 的测试。

#### External references / versions

- React: `packages/app-web/package.json` 声明 `react` / `react-dom` `^19.2.0`，lockfile 当前解析为 `19.2.4`，见 `packages/app-web/package.json:53`、`packages/app-web/package.json:54`、`pnpm-lock.yaml:2556`、`pnpm-lock.yaml:2567`。
- Zustand: `packages/app-web/package.json` 声明 `zustand` `^5.0.11`，见 `packages/app-web/package.json:59`。
- xterm: `@xterm/xterm` `^6.0.0`、`@xterm/addon-fit` `^0.11.0`、`@xterm/addon-web-links` `^0.12.0`，见 `packages/app-web/package.json:47`、`packages/app-web/package.json:48`、`packages/app-web/package.json:49`。
- Vitest: `packages/app-web/package.json` 声明 `vitest` `^4.0.18`，见 `packages/app-web/package.json:76`。

### 风险

1. History 输出丢失风险：用户刷新或打开旧 session 后，terminal tab 有 uri 但 store 没有 output buffer，显示为空；聊天 feed 也过滤了 terminal event，因此没有其它可见 fallback。
2. 状态漂移风险：`terminal_state_changed` 对未注册 terminal 静默无效；恢复历史 terminal tab 时 exit/lost/killed 状态和 exit code 可能丢失，状态栏错误显示 running。
3. 重复写入风险：live 当前通过 dispatcher 拦截避免 reducer 双写；若补 history hydrate 时直接在 reducer 中写 store，可能破坏 reducer 纯度并在 React/测试中引入重复副作用。应让 terminal hydrate 成为显式、幂等的单独通道。
4. 跳转不显眼风险：命令卡片直接写 tab store，不展开右侧 panel；用户点击后如果 panel 折叠，会认为没有跳转。
5. 跳错/丢 tab 风险：直接 store 写入绕过 `WorkspacePanel.openTab` 的 session/layout 上下文；若 WorkspacePanel 后续初始化或 session 切换重置 store，刚打开的 terminal tab 会消失。
6. Synthetic terminal 误导风险：`promote-*` tab 使用 xterm 且状态可能是 running，用户可以输入，但 input 会投递到不存在的真实 terminal id。
7. Stale promotion 风险：running command 的 promoted output 是点击时快照，不会继续随 command output 更新。
8. `/sessions/*` contract 风险：当前前端仍有多处 `/sessions/*` 直接调用。按本任务约束，不能把这些 endpoint 当作新修复方案；需要在后续切片中迁移到 AgentRun/runtime-facing contract 或 generated service 边界。

### 建议实现切片

1. Terminal history hydrate 切片
   - 在 `useSessionStream` history paging 后、设置 `historyReplayBoundarySeq` 前，增加明确的 terminal history projector/hydrator，对 `page.events` 中的 terminal platform events 顺序应用到 `useTerminalStore`。
   - 该 hydrator 不进入 `reduceStreamState`，保持 reducer 纯净；以 `sessionId + event_seq` 或 `terminal_id + event_seq` 做幂等，避免 reconnect/reset 时重复 append。
   - `terminal_output` 继续写 bounded buffer；`terminal_state_changed` 若 terminal 未注册，应创建最小 `TerminalInfo` 或写入 terminal-state shadow registry，保证历史 tab 能显示状态。若缺少 cwd/shell/processId，可使用 wire event 能证明的最小字段，不伪造真实 process 能力。

2. Terminal identity / capability 切片
   - 扩展 terminal store 的 terminal info，区分真实 interactive terminal 与 read-only output replay。当前 `TerminalInfo` 没有 capability 字段，导致 `promote-*` 与真实 terminal 共享同一 UI 行为。
   - 对 read-only output replay，TerminalView 不注册 `onData`/`onResize` POST，或在 UI 状态中禁用输入；标题/状态表达“命令输出”而非“运行中终端”。
   - 对真实 terminal，继续由 spawn result 或 backend terminal projection 注册 interactive capability。

3. Command card promotion 切片
   - 不再把“在终端中查看”实现为真实 terminal id。可以改为“查看完整输出”打开 read-only output tab，或保留 terminal tab 但以 `terminal://output/{item.id}` / structured URI 表达 read-only replay identity。
   - promotion output 应由 command item id 绑定到当前 `renderedOutput`，在 card output 变化时同步 read-only buffer，或打开时从 command entry/source 读取最新文本，避免 running 快照变 stale。
   - 按设计文案区分真实终端与命令输出，降低误导。

4. Workspace panel open 切片
   - 将打开 workspace panel 的能力从 `AgentRunWorkspacePage.expandWorkspacePanel` 注入到 `SessionChatView` / card body，或建立 session UI 的 workspace action context。
   - `CommandExecutionCardBody` 点击时调用页面级 `openWorkspacePanel({ typeId: "terminal", uri })`，而不是直接 `useWorkspaceTabStore.openOrActivate`。这样可以展开右栏，并走 `WorkspacePanel.openTab` 的 `tabLayoutOptions`。
   - 对非 AgentRun workspace 场景，提供显式 no-op/disabled 状态，而不是静默写全局 store。

5. `/sessions/*` replacement tracking 切片
   - 记录现存前端调用：`streamTransport.ts` 默认 `/api/sessions/{id}/stream/ndjson`，`services/session.ts` 的 events/meta/state/context/fork/lineage，`terminal-tab.tsx` 的 `/sessions/{id}/terminals`，`services/lifecycle.ts` 的 runtime-control/trace 等。
   - 本任务 UI/状态修复不要新增对这些端点的依赖；后续应迁移到 generated AgentRun/runtime workspace contract 或内部 service facade，避免组件直接拼路径。

### 建议测试

1. `useSessionStream` 或新 terminal history hydrator model test
   - 给定 history page 含 `terminal_output`，hydrate 后 `useTerminalStore.getOutput(terminal_id)` 有内容，`historyReplayBoundarySeq` 仍设置为最后 durable seq。
   - 给定 history page 多个 terminal output chunk，顺序 append，base offset 符合 256 KiB bounded policy。
   - 给定 history page 含 `terminal_state_changed` 且 terminal 未注册，hydrate 后能读取 terminal state/exit code，或至少 terminal tab 能显示 terminal state projection。
   - 给定 live terminal event，仍只通过 dispatcher 写一次 store，不进入 `rawEvents` 重复 side effect。

2. `sessionStreamReducer` regression test
   - 继续断言 terminal platform event 不进入 chat display entries，避免修复 history 时把终端噪音带回聊天 feed。

3. `TerminalView` / terminal tab model test
   - 打开已有 `terminal://id` 且 store 有历史 output 时，xterm write 从 offset 0 回放。
   - output buffer 裁切后，`lastWrittenOffsetRef` 与 `outputBaseOffset` 对齐，不重复写已裁切前缀。
   - read-only output terminal 不触发 `/terminals/{id}/input` / `/terminals/{id}/resize`。

4. `CommandExecutionCardBody` test
   - 点击“查看输出/在终端中查看”调用注入的 workspace panel open action，并传入具体 uri；不直接依赖全局 store 打开 panel。
   - running command output 变化后，read-only output tab 能看到最新输出，或再次打开会刷新到最新输出。
   - synthetic/read-only tab 不显示为可交互 running terminal。

5. `AgentRunWorkspacePage` integration/model test
   - command card open action 会展开右侧 panel 并激活目标 tab。
   - WorkspacePanel session 初始化后不会覆盖同一 session 内刚打开的 terminal/output tab。

6. Endpoint regression check
   - 本切片不新增新的 `/sessions/*` 调用点。
   - 对已存在 `/sessions/*` 调用加 TODO/issue 级追踪即可；不要把它们包装成新的推荐方案。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `.trellis/tasks/07-02-agent-parallel-wait-mailbox-implementation`，与用户消息指定的 active task `.trellis/tasks/07-02-terminal-output-navigation-repair` 不一致。本研究只写入用户指定且唯一允许的 research 文件。
- 本次只读调研前端生产代码，没有修改生产代码，也没有运行前端测试。
- 未调研后端 terminal cache / relay 生成 terminal events 的完整链路；本文件只覆盖前端展示、hydrate、store、tab、panel、command card promotion。
- 未发现现有代码把 history hydrate 的 terminal platform events 写入 terminal store 的路径。
- 未发现命令卡片 promotion 使用真实 terminal id 或后端 terminal projection 的证据；现有实现是 `promote-${item.id}` synthetic id + 当前输出文本快照。
