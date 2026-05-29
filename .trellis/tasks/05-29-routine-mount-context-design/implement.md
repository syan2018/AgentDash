# Routine mount 与跨轮次上下文实施计划

## Phase 0: Review Gate

- [x] 评审 `prd.md`、`design.md` 与本实施计划。
- [x] 确认 MVP 写入权限范围：有限开放，仅允许受控 Routine memory / 当前 entity memory 路径写入。
- [x] 确认 Routine memory 前端入口不进入本任务实现范围；协议支持通用 VFS Browser，Routine 页面入口作为后续前端备忘。
- [x] 确认 Routine 信息管理 skill 默认注入所有 Routine Session；是否实际使用由 prompt 与 Agent 当轮判断决定。

## Phase 1: Evidence Refresh

- [x] 读取后端规范：
  - `.trellis/spec/backend/architecture.md`
  - `.trellis/spec/backend/vfs/architecture.md`
  - `.trellis/spec/backend/vfs/vfs-access.md`
  - `.trellis/spec/backend/session/architecture.md`
  - `.trellis/spec/backend/session/session-startup-pipeline.md`
  - `.trellis/spec/backend/embedded-skill-bundles.md`
- [x] 读取相关代码：
  - `crates/agentdash-domain/src/routine/`
  - `crates/agentdash-application/src/routine/`
  - `crates/agentdash-application/src/session/construction_planner.rs`
  - `crates/agentdash-application/src/session/launch/command.rs`
  - `crates/agentdash-application/src/vfs/mount.rs`
  - `crates/agentdash-application/src/vfs/provider_lifecycle.rs`
  - `crates/agentdash-application/src/vfs/provider_skill_asset.rs`
- [x] 确认是否已有可复用 owner-based inline/context file repository。

## Phase 2: Domain And Persistence

- [x] 定义 Routine memory 持久化模型。
- [x] 复用现有 inline file storage，新增 `InlineFileOwnerKind::Routine`。
- [x] 不新增专用表；`inline_fs_files` 的 owner/container/path 模型可承载 Routine memory。
- [x] 新增 provider 单元测试覆盖 Routine 级与 entity 级 path 读写。

## Phase 3: VFS Provider

- [x] 新增 `PROVIDER_ROUTINE_VFS` 常量与 `build_routine_mount(...)` helper。
- [x] 将 `routine` 加入 reserved mount id / provider root validation。
- [x] 新增 `RoutineMountProvider`：
  - list `current/`
  - read `current/trigger.json`
  - read `current/execution.json`
  - read `current/resolved-prompt.md`
  - read/list/write `memory/*.md`
  - read/list/write 当前 `entities/{entity_key}/*.md`
- [x] 补 provider tests，覆盖只读路径与当前 entity path。

## Phase 4: Session Construction Integration

- [x] 扩展 Routine launch source metadata，携带 `routine_id`、`execution_id`、`trigger_source`、`entity_key`。
- [x] 在 Session construction 阶段识别 Routine source 并追加 Routine mount。
- [x] 确保 Routine projection 在 final VFS / capability projection 派生前注入。
- [x] 保持 `RoutineExecutor` 只负责 source intent 和 dispatch，不直接组装最终 VFS。

## Phase 5: Routine Memory Skill

- [x] 新增 embedded skill bundle `routine-memory`。
- [x] 编写 `SKILL.md`，说明 Routine memory 的读写顺序、归纳方式、运行结束更新方式。
- [x] 编写 `references/memory-model.md`，说明 `memory/` 与 `entities/` 的文件语义。
- [x] 通过统一 embedded skill materialization / SkillAsset projection 暴露 skill。
- [x] 将 `routine-memory` 作为 Routine Session 默认 skill baseline 注入。
- [x] 验证 Routine Session 可以发现该 skill，且普通非 Routine Session 不被该默认注入影响。

## Phase 6: API / Frontend Optional Scope

- [ ] 如首版不包含前端入口，在调试工具或 session context inspector 中确认 mount 可见。
- [x] 留下前端备忘：Routine 页面后续可增加“打开 Routine Memory”入口，跳转通用 VFS Browser 的 `routine://` mount。

## Validation Commands

后续实现时按实际改动选择验证：

```powershell
cargo test -p agentdash-domain routine
cargo test -p agentdash-application routine
cargo test -p agentdash-application vfs
cargo test -p agentdash-api routines
pnpm typecheck
pnpm test
```

如果修改 migration 或 repository：

```powershell
cargo test -p agentdash-infrastructure routine
```

如果修改前端 Routine 页面：

```powershell
pnpm test -- routine
pnpm dev
```

## Risky Files

- `crates/agentdash-application/src/session/launch/command.rs`
- `crates/agentdash-application/src/session/construction_planner.rs`
- `crates/agentdash-application/src/vfs/mount.rs`
- `crates/agentdash-application/src/vfs/path.rs`
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs`
- `crates/agentdash-api/src/bootstrap/background_workers.rs`
- `crates/agentdash-infrastructure/migrations/`

## Rollback Points

- Domain/persistence 可以独立回滚，只要 provider 未接入 construction。
- Provider 可以独立回滚，只要 mount 未进入 final VFS。
- Session construction 接入是主要集成点，提交前需要专门验证 final VFS 与 capability projection 一致。
- Skill bundle 可作为独立提交，因其不改变 Routine dispatch 主链路。
