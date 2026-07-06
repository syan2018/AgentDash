# Research: dangling legacy terminal/wait paths

- Query: 审计 AgentRun terminal convergence -> WaitProducerTerminalEvent(AgentRunDelivery) -> wait obligation convergence -> boot reconcile/preflight 成型后，是否仍有旧的、重复的、悬空的 terminal/gate/companion wait 路径保留旧事实源或旧抽象边界。
- Scope: internal
- Date: 2026-07-06

## Findings

### Files Found

- `.trellis/tasks/07-06-companion-subagent-terminal-gate-convergence/prd.md` - 任务目标和验收标准，明确 runtime terminal 必须先收敛为 AgentRun delivery fact，再进入 wait obligation convergence。
- `.trellis/tasks/07-06-companion-subagent-terminal-gate-convergence/design.md` - 设计边界，明确 `WaitProducerTerminalEvent` 外部 interface 不接受裸 `runtime_session_id` 或 gate kind。
- `.trellis/tasks/07-06-companion-subagent-terminal-gate-convergence/implement.md` - 实施计划，明确 `list_open_by_kind` / `list_by_agent_and_kind` 不再作为方案方向。
- `.trellis/tasks/07-06-companion-subagent-terminal-gate-convergence/check.jsonl` - check 上下文，包含 mailbox、activity lifecycle、contract 等复核规格。
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - 已记录 wait obligation terminal convergence 合同，要求 repository 按 wait producer 查询，不按 gate kind 扫描。
- `.trellis/spec/backend/session/agentrun-mailbox.md` - mailbox/waiting projection 合同，明确 waiting facts 由 gate/wait record 持有，mailbox message 只承载 wake/result envelope。
- `.trellis/spec/backend/session/execution-context-frames.md` - runtime session / frame 投影合同，说明 runtime session 是 connector 边界投影和 trace evidence。
- `.trellis/spec/backend/capability/llm-model-config.md` - SubAgent model preflight 合同，要求 dispatch 前按 effective provider/account 能力检查。
- `.trellis/spec/guides/code-reuse-thinking-guide.md` - 要求搜索已有 helper，重复三处以上才抽共享抽象。
- `crates/agentdash-api/src/agent_run_terminal_control.rs` - runtime terminal callback adapter。
- `crates/agentdash-application-agentrun/src/agent_run/terminal_convergence.rs` - AgentRun terminal convergence，产生 `AgentRunDeliveryTerminalEvent`。
- `crates/agentdash-application-workflow/src/gate/wait_obligation.rs` - wait obligation convergence 主实现。
- `crates/agentdash-domain/src/workflow/wait_obligation.rs` - wait producer / expected result / terminal policy / wake declaration。
- `crates/agentdash-domain/src/workflow/repository.rs` - `LifecycleGateRepository` 查询 surface。
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - PostgreSQL gate repository 查询实现。
- `crates/agentdash-application/src/reconcile/boot.rs` - boot reconcile wait obligation phase。
- `crates/agentdash-application/src/companion/dispatch.rs` - companion dispatch 创建 wait gate 后写入 wait obligation declaration。
- `crates/agentdash-application/src/companion/gate_control.rs` - companion normal result writer、wait obligation convergence adapter 和 mailbox wake delivery。
- `crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs` - wait tool 的 LifecycleGate source projection。
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs` - workspace waiting item projection model。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs` - workspace snapshot 查询 open gates。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - API workspace snapshot 追加 exec terminal waiting items。
- `crates/agentdash-application/src/companion/tools.rs` - companion_request / companion_respond / human wait runtime tool flow 与 model preflight 调用点。
- `crates/agentdash-api/src/bootstrap/session.rs` - effective provider/account model preflight adapter 注入点。

### Code Patterns

- 没有找到活跃代码中的 `list_open_by_kind` 或 `list_by_agent_and_kind`。当前 repository surface 是 `list_open_for_agent`、`list_open_wait_obligations`、`list_by_wait_producer`、`find_by_agent_and_correlation`，见 `crates/agentdash-domain/src/workflow/repository.rs:120`。
- PostgreSQL `list_open_wait_obligations` 只扫描 `status='open'` 且 payload 包含 `wait_source` / `expected_result` / `on_producer_terminal_without_result` 的 gate，未使用 gate kind，见 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:609`。
- PostgreSQL `list_by_wait_producer` 按 `payload_json.wait_source.kind/run_id/agent_id/frame_id` 查询，未使用 gate kind，见 `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:637`。
- `WaitProducerTerminalEvent` 的 public shape 是 producer ref + terminal diagnostics，不接受裸 `runtime_session_id`，见 `crates/agentdash-application-workflow/src/gate/wait_obligation.rs:17`。
- wait obligation convergence 从 `list_by_wait_producer` 取 gates，并只处理 `expected_result.kind == "companion_result"`，见 `crates/agentdash-application-workflow/src/gate/wait_obligation.rs:69` 和 `crates/agentdash-application-workflow/src/gate/wait_obligation.rs:86`。
- wait obligation convergence 对 open gate 通过 `LifecycleGateResolver::complete_child_result` 写入 result payload；对已 resolved gate 保留 payload 并生成 delivery ensure intent，见 `crates/agentdash-application-workflow/src/gate/wait_obligation.rs:101` 和 `crates/agentdash-application-workflow/src/gate/wait_obligation.rs:165`。
- terminal-derived payload 写入 `status`、`declared_status`、`terminal_state`、`terminal_message`、`delivery_trace_ref`、`failure_kind`、`source="producer_terminal"`，见 `crates/agentdash-application-workflow/src/gate/wait_obligation.rs:228`。
- API terminal callback 先调用 `AgentRunTerminalConvergenceService::converge_runtime_terminal`，再把 `AgentRunDeliveryTerminalEvent` 映射为 `WaitProducerTerminalEvent`，见 `crates/agentdash-api/src/agent_run_terminal_control.rs:55`、`crates/agentdash-api/src/agent_run_terminal_control.rs:89`、`crates/agentdash-api/src/agent_run_terminal_control.rs:122`。
- API terminal callback 中 `runtime_session_id` 只进入 AgentRun convergence command 或日志；wait producer event 用 `run_id/agent_id/frame_id` 定位，`delivery_trace_ref` 作为 trace ref，见 `crates/agentdash-api/src/agent_run_terminal_control.rs:79` 和 `crates/agentdash-api/src/agent_run_terminal_control.rs:125`。
- boot reconcile 先 `list_open_wait_obligations`，再用 `AgentRunDeliveryBindingRepository::get_current` 读取 producer terminal fact，并构造 `WaitProducerTerminalEvent`，见 `crates/agentdash-application/src/reconcile/boot.rs:189` 和 `crates/agentdash-application/src/reconcile/boot.rs:352`。
- boot reconcile 中 `binding.runtime_session_id` 只作为 `trace_ref` / diagnostic，不作为 gate 定位输入，见 `crates/agentdash-application/src/reconcile/boot.rs:435`。
- companion dispatch 创建 wait gate 后用 `WaitObligationDeclaration::companion_agent_run_delivery` 写入 payload，见 `crates/agentdash-application/src/companion/dispatch.rs:94` 和 `crates/agentdash-application/src/companion/dispatch.rs:230`。
- `WaitObligationDeclaration` 持有 `wait_source`、`expected_result`、terminal policy 和 `wake.client_command_id = companion-result:{gate_id}`，见 `crates/agentdash-domain/src/workflow/wait_obligation.rs:57` 和 `crates/agentdash-domain/src/workflow/wait_obligation.rs:64`。
- normal `companion_respond` path 仍从 `child_runtime_session_id -> RuntimeSessionExecutionAnchor -> child frame` 解析工具上下文，再用 `find_by_agent_and_correlation(child_agent_id, request_id)` 精确找 gate，并用 `companion_wait_follow_up` 过滤，见 `crates/agentdash-application/src/companion/gate_control.rs:641`、`crates/agentdash-application/src/companion/gate_control.rs:653`、`crates/agentdash-application/src/companion/gate_control.rs:673`。这是正常 result writer，不是 terminal convergence。
- `observe_wait_producer_terminal` 目前挂在 `CompanionGateControlService` 上，内部实例化 workflow 层 `WaitObligationConvergenceService` 并执行 companion mailbox delivery intents，见 `crates/agentdash-application/src/companion/gate_control.rs:807`。
- wait tool projection 对 resolved gate 读取 `LifecycleGate::resolved_payload_status()`，open gate 映射为 `pending`，见 `crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs:39`。
- workspace waiting projection 同样读取 `LifecycleGate::resolved_payload_status()`，但 open gate 保留原始 `open`，见 `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:215`。
- `LifecycleGate::resolved_payload_status()` 是当前共享的 resolved status helper，payload.status 缺失时 fallback 为 `completed`，见 `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:63`。
- workspace snapshot 只通过 `list_open_for_agent(agent.id)` 投影 open gates，resolved gate 不继续显示为 open waiting item，见 `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:193`。
- API 追加 exec terminal waiting items 来自 terminal registry，且只追加 `starting/running`，这是独立 source adapter，不是 LifecycleGate terminal convergence，见 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1640` 和 `crates/agentdash-api/src/routes/lifecycle_agents.rs:1674`。
- `waiting_kind_from_gate` / `waiting_kind_from_gate_kind` 在 wait activity 和 workspace projection 各有一份手写规则，见 `crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs:50` 和 `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:948`。
- SubAgent model preflight 在 dispatch 副作用前执行，见 `crates/agentdash-application/src/companion/tools.rs:883`。concrete adapter 使用 effective provider catalog，见 `crates/agentdash-api/src/bootstrap/session.rs:543`，并在 collaboration provider 上注入，见 `crates/agentdash-api/src/bootstrap/session.rs:629`。

### 可删除

- 未发现当前活跃代码里仍存在 `list_open_by_kind` 或 `list_by_agent_and_kind` helper/API；没有可直接删除的同名旧查询。
- 未发现 terminal callback 仍按 `companion_wait_follow_up` 扫 open gate 的旧路径；API callback 已经走 AgentRun delivery event -> wait producer event。
- 未发现 `child_owned` 作为活跃业务字段或 helper；只看到测试名里表达“child-owned gate”的场景语义，不构成旧路径。

### 应收束

- `CompanionGateControlService::observe_wait_producer_terminal` 仍作为 wait obligation terminal convergence 的 application port 实现，且 API terminal callback / boot reconcile 通过 companion service 间接调用 workflow 层 convergence。当前行为正确，但抽象边界还偏 companion：wait obligation convergence 的写侧和 delivery intent 执行可以进一步收束为 application-level wait obligation service，companion service 只提供 companion delivery adapter。证据：`crates/agentdash-api/src/agent_run_terminal_control.rs:93` 构造 `CompanionGateControlService`，`crates/agentdash-application/src/reconcile/boot.rs:41` 把 `CompanionGateControlService` 实现为 `WaitObligationTerminalConvergencePort`，`crates/agentdash-application/src/companion/gate_control.rs:807` 内部再转调 `WaitObligationConvergenceService`。
- wait tool 和 workspace projection 的 kind/source label/preview 规则仍是两套手写 helper。resolved status 已经收束到 `LifecycleGate::resolved_payload_status()`，但 kind mapping 仍重复；如果后续新增 gate kind，两个 projection 容易漂移。证据：`crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs:50` 与 `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:948`。
- `find_by_agent_and_correlation` 是正常 companion result writer 的精确查询，应保留；但它同时是 repository trait 上的通用方法，容易被未来 terminal/reconcile 误用成“按 agent/correlation 找 gate”的旧边界。最小收束不是删除，而是把调用限制在 normal result writer，并用测试/注释或更窄命名表达它不是 terminal convergence interface。证据：repository trait `crates/agentdash-domain/src/workflow/repository.rs:132`，唯一关键生产调用在 `crates/agentdash-application/src/companion/gate_control.rs:673`。

### 应保留

- `companion_wait_follow_up` gate kind 应保留为 compatibility/projection 标签和 normal writer 过滤条件。它不再是 terminal convergence 的外部定位输入；producer terminal 查找已经转为 payload `wait_source`。证据：gate kind 创建在 `crates/agentdash-application/src/companion/dispatch.rs:280`，terminal convergence 查询在 `crates/agentdash-application-workflow/src/gate/wait_obligation.rs:73`。
- `list_open_for_agent` 应保留。它是 workspace/wait open projection 查询，不参与 terminal resolve；workspace 只用它展示 open waiting items。证据：repository trait `crates/agentdash-domain/src/workflow/repository.rs:123`，workspace caller `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:193`。
- `list_open_wait_obligations` 应保留。它是 boot/retry bounded scan，只扫描声明了 wait obligation policy 的 open gates，不按 kind 扫描。证据：`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:609`，boot caller `crates/agentdash-application/src/reconcile/boot.rs:195`。
- `list_by_wait_producer` 应保留。它是 runtime callback/replay 的精确 producer fact 查询，按 wait_source producer 匹配，支持已 resolved gate delivery ensure。证据：`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:637`，convergence caller `crates/agentdash-application-workflow/src/gate/wait_obligation.rs:73`。
- `runtime_session_id` 在 companion runtime tools 中应保留为工具当前执行上下文 / delivery trace ref。normal `companion_respond`、parent request、human wait 需要从当前 runtime session 解析 AgentRun frame 并验证 current delivery；这不是 terminal convergence 的事实源泄漏。证据：`crates/agentdash-application/src/companion/gate_control.rs:653`、`crates/agentdash-application/src/companion/gate_control.rs:887`、`crates/agentdash-application/src/companion/gate_control.rs:1043`。
- API exec terminal waiting projection 应保留。它是 terminal registry 的独立 source adapter，只追加 running/starting terminal items，且不和 LifecycleGate wait obligation 混成同一事实源。证据：`crates/agentdash-api/src/routes/lifecycle_agents.rs:1640`。
- `companion_wait_payload_status` / companion notification payload status readers应保留为 tool response / UI event formatting helper；它们不写 durable facts，也不参与 gate resolve。证据：`crates/agentdash-application/src/companion/tools.rs:288`，`crates/agentdash-application/src/companion/notifications.rs:27`。
- SubAgent model preflight 应保留。它已经在 dispatch 副作用前执行，避免已知配置错误留下长期 wait gate；runtime-only provider 400 仍由 terminal convergence 兜住。证据：`crates/agentdash-application/src/companion/tools.rs:885` 和 `crates/agentdash-api/src/bootstrap/session.rs:543`。

### Suggested Minimal Implementation Slice

1. 不做大规模删除；当前没有发现可直接删除的活跃旧 terminal/gate 查询。
2. 抽一个小的 LifecycleGate waiting projection helper，至少统一 kind mapping、source label key priority、preview key priority；保留 `LifecycleGate::resolved_payload_status()` 作为 status helper。先让 wait activity 和 conversation snapshot 复用它，或用共享测试锁住两者一致性。
3. 将 `WaitObligationTerminalConvergencePort` 的 concrete owner 从 `CompanionGateControlService` 逐步提升到 application wait-obligation service：service 组合 workflow `WaitObligationConvergenceService` + delivery intent executor；companion service 保留 normal writer 和 delivery adapter。第一步可以只新增 facade 并改 API callback/boot reconcile 注入该 facade，不改变 payload/result 行为。
4. 给 `find_by_agent_and_correlation` 加 focused regression/static check：terminal callback 和 boot reconcile 不得调用它；它只服务 normal result writer / exact correlation retry。

### Risk

- 把 `companion_wait_follow_up` 当作“名字旧所以删除”会破坏 projection 分类和 normal `companion_respond` 精确匹配；它现在不是 terminal convergence 的事实源。
- 过早删除 `runtime_session_id` 工具上下文会破坏 companion tool 对当前 AgentRun frame/current delivery 的校验；需要区分 runtime tool scope 解析和 terminal convergence business address。
- 如果只抽 projection kind helper而不补测试，后续新增 wait kind 仍可能在 API exec terminal adapter、wait tool、workspace projection 三处漂移。
- 将 convergence owner 从 companion service 上提时要保留 parent mailbox delivery 的稳定 dedup key `companion-result:{gate_id}`，否则 resolved gate replay 可能丢 wake 或重复投递。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本研究按用户显式给出的 `.trellis/tasks/07-06-companion-subagent-terminal-gate-convergence` 写入。
- 未运行测试；本次是只读研究审计。
- 未发现同名 `list_open_by_kind` / `list_by_agent_and_kind` 活跃实现。
- 未发现 API terminal callback 直接查询 `RuntimeSessionExecutionAnchorRepository` 来 resolve companion gate；anchor 解析集中在 AgentRun terminal convergence 或 runtime tool scope 解析路径。
- External references: none. 本审计只使用仓内任务文档、spec 和代码。
- Related specs: `.trellis/spec/backend/workflow/activity-lifecycle.md`、`.trellis/spec/backend/session/agentrun-mailbox.md`、`.trellis/spec/backend/session/execution-context-frames.md`、`.trellis/spec/backend/capability/llm-model-config.md`、`.trellis/spec/guides/code-reuse-thinking-guide.md`。
