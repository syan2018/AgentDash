# Lifecycle 控制面长链路收敛与 Frame 化 Implement Plan

## Recommended Start Order

1. `06-02-runtime-session-frame-assignment-anchor`
   - 先建立 runtime session 到 frame / assignment / attempt 的权威锚点。
   - 这是 terminal callback、frontend frame-runtime endpoint、ContinueRoot reused runtime session 的共同前置。
2. `06-02-scoped-lifecycle-artifacts`
   - 在 attempt anchor 稳定后，把 output port / hook gate / completion gate 迁到 graph/activity/attempt scoped artifact。
   - 可与第 3 阶段前端查询并行，但实现时不要依赖前端先完成。
3. `06-02-frontend-session-runtime-frame-query`
   - 复用第 1 阶段 anchor endpoint，删除本地 cache-first frame 推断。
   - 若第 2 阶段已完成，则 runtime view 可以同步消费更可靠的 attempt/artifact projection。
4. `06-02-frame-launch-envelope-session-boundary`
   - 最大、最中心的重构项，放在 anchor 与 artifact scope 之后，避免在 Session launch 中央路径和 terminal/artifact 路径同时大改。
   - 目标是让 Session planner 只消费 launch-ready `FrameLaunchEnvelope`。
5. `06-02-lifecycle-run-active-projection-structure`
   - 尾部清理 run-level 字符串 active projection。
   - 当前前端业务不直接消费 `active_node_keys`，后端业务路径迁出后再结构化最稳。
6. Parent final integration
   - 汇总 specs、contracts、migrations、read model，不再允许 session-first / run-first fallback 回流。

## Phase 0: Audit And Baseline

- [ ] 读取 backend workflow/session/permission/capability/frontend specs，确认本任务触及层级的不变量。
- [ ] 列出所有 `find_by_runtime_session`、`runtime_session_refs_json`、`load_port_output_map`、`RuntimeLaunchRequest`、`current_frame_id` 的生产路径。
- [ ] 为每条路径标注事实源：Frame surface、Assignment anchor、Runtime trace、Session turn supervision、read model。

## Phase 1: RuntimeSession To Frame / Assignment Anchor

- [ ] Child task: `.trellis/tasks/06-02-runtime-session-frame-assignment-anchor`
- [ ] 设计并实现 runtime session 到 frame / assignment 的直接锚定查询或实体。
- [ ] 改造 terminal callback 与 `complete_lifecycle_node`，避免 run 级 assignment 列表扫描和启发式 fallback。
- [ ] 补充多 assignment / frame revision / reused agent 场景测试。

## Phase 2: Scoped Lifecycle Artifacts

- [ ] Child task: `.trellis/tasks/06-02-scoped-lifecycle-artifacts`
- [ ] 设计 scoped port artifact key：`graph_instance_id + activity_key + attempt + port_key`。
- [ ] 更新 lifecycle VFS write/read、journey service、hook gate、completion policy、artifact binding。
- [ ] 添加 migration，移除运行时旧 path 兼容。
- [ ] 补充多 graph instance 同名 port 的集成测试。

## Phase 3: Frame Launch Envelope

- [ ] Child task: `.trellis/tasks/06-02-frame-launch-envelope-session-boundary`
- [ ] 拆分 `RuntimeLaunchRequest` 的职责，建立 launch-ready `FrameLaunchEnvelope`。
- [ ] 将 owner/context/capability/VFS/MCP/execution profile 解析上提到 Frame construction。
- [ ] 收窄 Session launch planner，使其只消费 envelope 并处理 turn/connector/stream/terminal。
- [ ] 明确 `LifecycleAgent.current_frame_id` 与 frame current revision 的唯一不变量并补测试。

## Phase 4: Frontend Runtime Query

- [ ] Child task: `.trellis/tasks/06-02-frontend-session-runtime-frame-query`
- [ ] 提供或扩展 backend endpoint：由 `session_id` 返回 `AgentFrameRuntimeView` / trace，推荐 `GET /sessions/{runtime_session_id}/frame-runtime`。
- [ ] 改造 `useSessionRuntimeState`，去除本地 frame cache fallback。
- [ ] 更新 lifecycle store 派生查询与测试。

## Phase 5: Run-Level Projection Cleanup

- [ ] Child task: `.trellis/tasks/06-02-lifecycle-run-active-projection-structure`
- [ ] 将 `LifecycleRun.active_node_keys` 改为结构化 read projection 或从业务路径移除。
- [ ] 明确 `lifecycle_id` 的目标命名与 root graph instance 投影关系。
- [ ] 更新 DTO / generated TS / frontend 消费点。

## Validation Commands

- [ ] `pnpm run contracts:check`
- [ ] `cargo check`
- [ ] `cargo test -p agentdash-application`
- [ ] `cargo test -p agentdash-infrastructure`
- [ ] `pnpm --filter app-web test`
- [ ] 根据实际改动补充 focused e2e 或 route integration test。

## Risk Points

- Frame revision current 语义需要一次性定清，否则会继续形成隐性双事实源。
- Artifact scope migration 会影响 hook gate、lifecycle VFS、completion policy、read model 多处路径。
- Session launch planner 收窄时要保留 turn supervision、terminal cleanup、backend lease 的运行职责。
