# 实施计划

实现分支：`codex/local-app-runtime-integration`。该分支被视为完善 local app 的长程分支，承接先前桌面样式工程化工作，并继续完成本机 runtime、server 注册闭环、桌面信息架构和共享样式下沉。

提交应按阶段拆分，避免把 server 协议、Tauri 生命周期和 UI 重构压进一个大提交。移动组件时优先 `git mv`，保留历史。

## Phase 0: 准备与保护网

- 重新跑 `pnpm check` 或项目现有类型检查，记录当前基线。
- 梳理 web Settings 入口、desktop app bootstrap、local runtime manager、backend repository 的测试入口。
- 给当前 Desktop 双视图和 runtime_start 行为补最小 characterization test，避免重构时失去行为锚点。

## Phase 1: Server ensure API 与数据库迁移

范围：

- `crates/agentdash-infrastructure/migrations`
- `crates/agentdash-domain/src/backend`
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs`
- `crates/agentdash-api/src/routes`

工作：

- 增加 `backends.profile_id/device_id/device/last_claimed_at` 迁移与 local backend 唯一索引。
- 扩展 `BackendConfig` 或新增 `LocalRuntimeClaim` domain type。
- 新增 repository 方法：按 `owner_user_id + profile_id + device_id` ensure local backend。
- 新增 `POST /api/local-runtime/ensure` route。
- API 返回 `backend_id/auth_token/relay_ws_url/profile_id/backend_enabled`。
- 保留 `/ws/backend` 的强校验逻辑。

验证：

- Rust 单测覆盖新建、复用、用户隔离、token rotate、重复 token 防护。
- `cargo test -p agentdash-api -p agentdash-infrastructure`。

提交建议：

- `feat(server): 增加本机运行时领取接口`

## Phase 2: Tauri profile 与自动启动闭环

范围：

- `crates/agentdash-local-tauri/src/main.rs`
- 可能新增 `crates/agentdash-local-tauri/src/desktop_profile.rs`
- `crates/agentdash-local/src/runtime.rs` 仅做必要输入结构调整

工作：

- 将单一 `desktop-runtime-profile.json` 迁移为按 server origin 隔离的 desktop profile store。
- 新增 Tauri command：
  - `desktop_profile_snapshot`
  - `desktop_profile_update`
  - `runtime_ensure_and_start`
  - `runtime_reset_profile`
  - `runtime_rotate_token` 如 server API 支持
- `runtime_start` 不再在缺少 backend_id 时生成随机 UUID。缺少 server-issued backend_id 应报错或走 ensure path。
- Desktop API ready 后，根据 active profile 和 auto_start 调用 ensure 并启动 local runtime。
- server target 切换时停止旧 runtime，切换 profile 后重新 ensure/start。
- 将 ensure 错误、WS 错误、duplicate online 映射为结构化状态。

验证：

- Tauri Rust 单测覆盖 profile key、server origin 隔离、随机 backend_id 删除。
- local runtime manager 单测覆盖 ensure failure 不启动。
- 手测 `pnpm dev` 启动后 runtime 自动 online。

提交建议：

- `feat(desktop): 串联本机运行时自动注册启动`

## Phase 3: Desktop App 外壳改造

范围：

- `packages/app-tauri/src/App.tsx`
- `packages/app-tauri/src/components/*`
- 可能移动 desktop client/provider 到共享前端包

工作：

- 移除 `DesktopView = 'runtime' | 'dashboard'` 顶层切换。
- 保留/移动 embedded API ready 逻辑到 Desktop provider/bootstrap。
- Desktop App 只渲染 shared web app 主体验。
- 注入 desktop capabilities adapter，供 Settings Local 面板消费。
- 删除或迁移旧 `DashboardHost` 与 standalone `LocalRuntimeView`。

验证：

- TypeScript check。
- Desktop 启动后首屏是统一 web dashboard，不出现旧左侧 Runtime/Dashboard 双导航。

提交建议：

- `refactor(desktop): 合并桌面端主应用入口`

## Phase 4: Settings Local Runtime 面板

范围：

- `packages/app-web` Settings 相关目录
- `packages/app-tauri` desktop adapter
- `packages/ui` 共享组件

工作：

- 在 Settings 增加 desktop-only Local Runtime tab。
- 从旧 LocalRuntimeView 移动可复用逻辑，拆成 shared hooks/components：
  - `useLocalRuntimeStatus`
  - `LocalRuntimeStatusPanel`
  - `LocalRuntimeRootsEditor`
  - `LocalRuntimeDiagnostics`
  - `RuntimeLogsViewer`
- 使用共享 UI 组件重做布局，遵循前序样式工程化决策。
- Web 环境隐藏 desktop-only tab。

验证：

- Playwright/browser 截图验证 desktop settings 样式与 web app 一致。
- 状态：starting、online、stopped、error、server mismatch、duplicate online。

提交建议：

- `feat(frontend): 增加桌面本机运行时设置面板`

## Phase 5: 状态融合与实时刷新

范围：

- runtime health queries/hooks
- desktop adapter
- server websocket/event invalidation

工作：

- 对本机 backend 的 server health query 加轮询或 websocket invalidation。
- local IPC 状态变化时局部更新 query cache，类似 multica 的 `useDaemonIPCBridge`，但只加速本机 backend 显示，不替代 server 权威。
- Local 面板展示冲突状态：
  - local running + server offline
  - local stopped + server online
  - token invalid
  - backend disabled

验证：

- Stop/Start 后 UI 即时变化。
- server health 最终一致。

提交建议：

- `feat(frontend): 融合本机与服务端运行状态`

## Phase 6: 清理旧路径与文档/spec 更新

范围：

- 删除旧 standalone runtime 页面入口。
- 更新 `.trellis/spec/cross-layer/desktop-local-runtime.md`。
- 更新开发文档与 task 记录。

工作：

- 移除手动三元组默认路径。
- 确认没有第二套 CSS 入口。
- 更新 spec：Desktop 是 web app + local capability provider，Local Runtime 位于 Settings。
- 记录迁移后的启动流程和测试命令。

验证：

- `rg "DashboardHost|DesktopView|desktop-runtime-profile|RuntimeStartRequest.*backend_id.*unwrap_or_else"` 不再命中旧问题模式。
- `pnpm check`、Rust tests、必要 Playwright 截图。

提交建议：

- `docs(spec): 更新桌面本机运行时闭环规范`

## 风险与注意事项

- 不要在 Tauri 侧自造 token 或 backend_id；这会绕过 server 权威，继续制造当前问题。
- 不要把 Local Runtime 做成另一个 app 页面；它是 Desktop Settings 能力。
- 不要复制 web dashboard 到 app-tauri；应移动/抽取组件。
- server target 切换必须先 stop old runtime，否则可能出现一个桌面进程同时向两个 server 报在线。
- duplicate online 不应静默抢占，除非协议明确支持 server-side takeover。
- 迁移 `backends` 时要检查已有 local backend 的 owner_user_id 为空情况，预研期可以用一次性数据修正让状态正确。
