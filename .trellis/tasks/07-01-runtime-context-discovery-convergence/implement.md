# Runtime context discovery 单入口收束执行计划

## Phase 1: Envelope Boundary

- 审计 `FrameLaunchEnvelope` 顶层字段，把现有字段归类为：
  - frame refs
  - command intent
  - runtime surface
  - context projection
  - diagnostics
- 引入内部结构或重命名现有结构，先完成语义分组，不改变外部行为。
- 更新 `LaunchPlan`、`TurnPreparer`、connector launch 调用点，改为通过分组对象读取事实。
- 保持一层兼容性最小的内部 accessor（如有必要），但不继续新增顶层平铺字段。
- 增加 focused tests，确保 envelope 重组前后 launch plan 关键字段一致。

## Phase 2: Discovery Path Audit

- 列出所有 `FrameLaunchEnvelope` 构造路径：
  - ProjectAgent owner compose
  - LifecycleNode compose
  - ExistingSurface
  - companion owner modifier
  - workflow/lifecycle continuation
- 标注每条路径当前如何设置：
  - `FrameLaunchIntent.discovered_guidelines`
  - `FrameLaunchIntent.discovered_memory`
  - skill baseline / capability state
- 搜索并记录重复 scanner helper：
  - `should_scan_mount_for_discovery`
  - `list_recursive_files`
  - `try_read_*_vfs_file`
  - `AUTO_DISCOVERY_METADATA_KEY`

## Phase 3: Shared Discovery API

- 在 VFS/application 边界定义统一 scanner adapter API。
- 将 Skill VFS discovery 从 `agentdash-application-skill` 的重复 scanner 迁移到 `agentdash-application-vfs::mount_file_discovery`。
- 保持 Skill frontmatter parse / validation 留在 `agentdash-application-skill`。
- 为 guideline/memory/skill adapters 增加 focused unit tests。

## Phase 4: Launch-Time Orchestration

- 引入 `LaunchContextDiscoveryInput/Output`。
- 在 `FrameLaunchEnvelope` 构造闭包最终 runtime surface 后运行 discovery。
- 将 discovery output 写入 envelope 的 context projection 分组。
- 清理 route 层手动填充 discovered context 的逻辑。
- ExistingSurface route 必须覆盖 AGENTS.md discovery。

## Phase 5: ContextFrame Verification

- 增加整链路测试：
  - VFS 中存在 `AGENTS.md`
  - launch intent 带 `DiscoveredGuideline`
  - launch plan 带 discovered guidelines
  - prepared context frames 包含 `system_guidelines`
  - delivery metadata 为 `session_policy #20`
- 保持 memory context 与 skill/capability delta 测试通过。

## Phase 6: Frontend And Spec

- 确认前端 ContextFrame stream/list 能展示 `system_guidelines` 与 `identity` 并存。
- 更新 `.trellis/spec/backend/session/execution-context-frames.md` 或 VFS/session appendix，记录 `FrameLaunchEnvelope` 分层与 runtime context discovery 单入口原因。

## Validation Commands

```powershell
cargo test -p agentdash-application-vfs mount_file_discovery -- --nocapture
cargo test -p agentdash-application-agentrun runtime_capability_projection -- --nocapture
cargo test -p agentdash-application-runtime-session guidelines_context_frame -- --nocapture
cargo test -p agentdash-application-runtime-session delivery_plan_orders_frames_and_keeps_pi_memory_out_of_system -- --nocapture
cargo test -p agentdash-application frame_construction -- --nocapture
cargo check -p agentdash-application -p agentdash-application-skill -p agentdash-application-agentrun -p agentdash-application-runtime-session
pnpm --filter app-web test -- contextFrame.test.ts ContextFrameCard.test.tsx
```
