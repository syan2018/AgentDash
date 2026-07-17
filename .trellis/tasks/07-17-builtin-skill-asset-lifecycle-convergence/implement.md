# 实施计划

1. 收敛 SkillAsset provisioning
   - 将 `bootstrap_builtins` 重命名为显式 project provisioning。
   - 增加全集幂等与 builtin mutation 保护测试。
   - 在新建 Project、clone Project 和 API repository bootstrap 接入全集
     provisioning。
2. 纯化 lifecycle projection
   - 将 builtin policy hard cut 为 `Project` / `PreserveProjected`。
   - 删除 bootstrapper trait 和 projection 写调用。
   - 使用只读 repository 验证最终 keys，缺失时显式失败。
3. 修复首次 frame 坐标链
   - 让 `AgentFrameBuilder` 预分配并暴露稳定 frame ID。
   - ProjectAgent composition 直接传递 runtime session 与 builder frame ID。
   - 删除 owner bootstrap 的 runtime binding 反查和无 projection fallback。
4. 收敛 Assets 操作语义
   - 后端拒绝 builtin update/delete。
   - 前端 builtin 卡片改为查看且移除删除动作。
5. 回归与检查
   - 定向格式化修改的 Rust 文件。
   - 运行 application-skill、application-lifecycle、application frame
     construction 的定向测试。
   - 运行 API/前端受影响检查，避免重复跑无关大测试。
6. 更新 `embedded-skill-bundles.md`，记录 catalog、provisioning 与纯 projection
   的可执行契约；完成 Trellis check、提交和归档。
7. 收敛 Workspace Module presentation 前端消费
   - 用真实 journal 记录验证 typed success event 已持久化到当前 AgentRun。
   - 将 `workspace_module_presented` 从通用静默投影分类为可渲染成功事件。
   - 让 concrete presentation URI 立即打开 panel，移除 presentation target 上含混
     的 runtime refresh 标记。

## 风险与回滚点

- `AgentFrameBuilder` frame ID 时序是跨 runtime surface 的关键事实；必须用单测锁定
  build 前后同一 ID。
- lifecycle policy variant 是 workspace 内部 hard cut，必须 `rg` 更新全部调用点。
- repository bootstrap 会遍历 Project；失败必须带 project ID，且不吞错。
- 本任务不触碰当前工作区之外的并行修改；若出现外部 dirty path，立即停止覆盖。

## 验证命令

```powershell
cargo test -q -p agentdash-application-skill skill_asset::service::tests
cargo test -q -p agentdash-application-lifecycle lifecycle::surface
cargo test -q -p agentdash-application-agentrun agent_run::frame::builder::tests
cargo test -q -p agentdash-application-shared-library
cargo test -q -p agentdash-api bootstrap::repositories::tests
cargo check -q -p agentdash-api -p agentdash-application -p agentdash-application-agentrun -p agentdash-application-lifecycle -p agentdash-application-skill -p agentdash-application-shared-library
pnpm --filter app-web test -- skillAssetCardPolicy
pnpm --filter app-web test -- src/features/agent-run-workspace/model/controlPlaneModel.test.ts src/features/session/model/platformEvent.test.ts src/features/session/ui/SessionSystemEventCard.test.tsx src/pages/AgentRunWorkspacePage.workspace-module.test.ts
pnpm --filter app-web run typecheck
rg -n "EnsureAndProject|BuiltinLifecycleSkillBootstrapper|resolve_runtime_surface_refs" crates
```

## 实施结果

- Project create/clone 与 API bootstrap 已统一调用 catalog 全集 provisioning；启动
  reconciliation 的内存仓储回归覆盖多 Project 与重复执行 identity。
- lifecycle projector 已移除 Skill service 写依赖，只读取仓储、验证最终 keys 并投影
  metadata；缺失资产返回 Project ID/key。
- `AgentFrameBuilder` 预分配 identity 且为单次消费；Project owner 首次 composition
  直接使用 run/agent/frame/runtime session 坐标。
- builtin 的 update、delete、upload overwrite、library install overwrite 与 publish
  边界均已收紧，Assets 前端呈现只读查看语义。
- Rust 定向格式化、组合 `cargo check`、关键单元/集成测试、Shared Library 29 项、
  前端 policy 2 项与 TypeScript typecheck 通过。
- 真实开发数据库中的成功事件已验证为同一 Runtime thread 上的 canonical
  `control_plane_projection_changed(reason=workspace_module_presented)`；前端已改为
  immediate panel open，并渲染带模块/视图/渲染器信息的成功卡片。新增回归先稳定复现
  `afterWorkspaceRefresh=true` 与静默分类，再修复至 44 项定向测试通过。
- 受影响 Rust 包的严格 Clippy 被仓库既有 `large_enum_variant`、
  `unnecessary_filter_map` 与 API `let_and_return` 告警阻断；新增代码未引入 suppress。
  `SkillCategoryPanel.tsx` 全文件 ESLint 同样命中两处既有
  `react-hooks/set-state-in-effect`，新增 policy 文件定向 lint 通过。
