# Implementation Plan

## Parallel Strategy

先做 1 个短串行底座，然后拆成 5 条可并行工作线，最后做集成收口。

```text
0. crate skeleton / move map
  ├─ A. skill crate extraction
  ├─ B. lifecycle builtin ensure
  ├─ C. VFS-first discovery restore
  ├─ D. caller cleanup and wiring
  └─ E. test harness and regression specs
        ↓
6. integration pass / checks / spec update
```

## Step 0: Serial Bootstrap

- 新增空的 `crates/agentdash-application-skill`，加入 workspace 和 workspace dependency 表。
- 建立最小 `lib.rs` 模块骨架：`asset`, `builtin`, `discovery`, `baseline`, `error`。
- 在总 `agentdash-application` 中临时 re-export 新 crate 类型，给并行改动提供稳定 import 目标。
- 只做可编译骨架，不迁移行为；完成后各工作线可并行。

Validation:

- `cargo check -p agentdash-application-skill`
- `cargo metadata --format-version=1 --no-deps`

## Track A: Skill Crate Extraction

Owner: crate split / compile closure.

- 将 `agentdash-application/src/skill` 迁入 `agentdash-application-skill/src/discovery` 或 `loader`。
- 将 `agentdash-application/src/skill_asset` 迁入 `agentdash-application-skill/src/asset` 与 `builtin`。
- 保留总 `agentdash-application::skill` / `skill_asset` re-export，减少 API route 一次性改动。
- 调整 tests import，确保原 `skill_asset::service::builtin_bootstrap_*` 在新 crate 内通过。

Validation:

- `cargo test -p agentdash-application-skill skill`
- `cargo test -p agentdash-application-skill skill_asset`

## Track B: Lifecycle Builtin Ensure

Owner: lifecycle surface projector.

- 让 `agentdash-application-lifecycle` 依赖 `agentdash-application-skill`。
- 在 `AgentRunLifecycleSurfaceProjector` 中实现 `EnsureAndProject` 的 ensure 阶段：对 builtin keys 调 `SkillAssetService::bootstrap_builtins(project_id, Some(key))`。
- `PreserveProjected` 只读已有 projection，不触发 bootstrap。
- 删除或废弃 `skill_asset_repo` 未读状态，改为真实服务依赖。
- 增加 lifecycle projector 测试：`CanvasSystem` / `WorkspaceModuleSystem` / `CompanionSystem` 都会创建项目 SkillAsset 并刷新 metadata。

Validation:

- `cargo test -p agentdash-application-lifecycle lifecycle::surface`

## Track C: VFS-First Discovery Restore

Owner: skill baseline / provider discovery.

- 将 skill VFS scanning 与 provider output normalization 放到 `agentdash-application-skill`。
- 对有 `vfs_discovery_rules()` 的 provider：使用 active VFS + `VfsService` 扫描，再走 provider 的 VFS discovery path 或等价转换。
- 对无 VFS rules 的 provider：继续调用 `discover(context)`。
- 删除正常路径上的 `vfs_scanner_unavailable` diagnostic。
- 增加测试 provider，证明 VFS-first provider 可从 lifecycle VFS 发现 skill。

Validation:

- `cargo test -p agentdash-application-skill vfs`
- `cargo test -p agentdash-application-skill baseline`

## Track D: Caller Cleanup And Wiring

Owner: frame construction / application wiring.

- 删除 `ensure_companion_system_skill_asset`、`append_companion_system_skill_key` 这类 companion-only 注入 helper。
- owner bootstrap、request assembler、activity activation、companion child、routine/workspace module 入口统一声明 `BuiltinLifecycleSkillPolicy::EnsureAndProject`。
- frame construction / runtime surface update 使用 skill crate 的 baseline projection。
- 确认 `CapabilityState.skill.skills` 从 final active VFS 派生，不从 key 列表直接伪造。

Validation:

- `cargo test -p agentdash-application skill`
- `cargo test -p agentdash-application frame_construction`
- `cargo test -p agentdash-application-agentrun skill`

## Track E: Regression Tests And Fixtures

Owner: tests and diagnostics.

- 增加端到端-ish 单元测试：projector ensure 后，`lifecycle://skills/canvas-system/SKILL.md` 可读并进入 baseline。
- 增加 companion 回归测试：companion system skill 走统一 builtin lifecycle policy。
- 增加 workspace module/canvas 回归测试：`canvas-system` 与 `workspace-module-system` 同时可见。
- 增加 diagnostic 断言：正常 VFS-first provider 不产生 `vfs_scanner_unavailable`。

Validation:

- 随 Track B/C/D 对应 crate 测试一起跑。

## Integration Pass

- 移除总 `agentdash-application` 中已无用的旧模块实现，只保留必要 public re-export。
- 清理 dead imports、Cargo 依赖和旧 helper 测试。
- 更新 backend spec 中 embedded skill bundle / capability pipeline 相关文字，记录 skill crate 和 runtime bootstrap 事实源。
- 跑完整质量门。

## Validation Commands

- `cargo fmt --check`
- `cargo test -p agentdash-application-skill`
- `cargo test -p agentdash-application-lifecycle lifecycle::surface`
- `cargo test -p agentdash-application-agentrun skill`
- `cargo test -p agentdash-application skill`
- `cargo check -p agentdash-api`
- `cargo check --workspace`

## Risk Points

- Crate split 可能暴露循环依赖；如果出现循环，调整边界时优先让 skill crate 只依赖 `domain`、`spi`、`application-vfs`、`application-ports`，不得依赖总 `application`、`application-lifecycle` 或 `application-agentrun`。
- VFS-first provider 的 trait 方法若不够表达扫描文件输入，需要在 `agentdash-spi` 中补明确默认方法，并同步测试 provider。
- 不引入 marketplace fallback；runtime builtin skill 的事实源是 embedded bundle + project SkillAsset。
