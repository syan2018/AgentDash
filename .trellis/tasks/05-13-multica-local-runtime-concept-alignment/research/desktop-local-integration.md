# Desktop 与本机能力合并

## multica desktop 结构

关键目录：

- `references/multica/apps/desktop/src/main`：Electron main，包含 daemon manager、CLI bootstrap、updater、runtime config loader。
- `references/multica/apps/desktop/src/preload`：IPC bridge 类型与暴露。
- `references/multica/apps/desktop/src/renderer`：desktop renderer app、tab shell、daemon panel、runtime page。
- `references/multica/packages/core`：查询、mutation、类型、realtime sync、stores。
- `references/multica/packages/views`：业务页面，跨 web/desktop 复用。
- `references/multica/packages/ui`：通用 UI 原子组件。
- `references/multica/apps/web`：Next app shell。

## 与 AgentDash 当前前端对比

| 维度 | AgentDash | multica |
| --- | --- | --- |
| App shell | `frontend/src/App.tsx`, pages | `apps/web/app`, `apps/desktop/src/renderer/src` |
| 业务视图 | `frontend/src/features` | `packages/views` |
| API/query | `frontend/src/services`, `frontend/src/api` | `packages/core/*/queries.ts`, `mutations.ts` |
| Client state | `frontend/src/stores` | `packages/core` stores + desktop renderer stores |
| UI kit | `frontend/src/components/ui` | `packages/ui` |
| Desktop main/preload | 暂无 | `apps/desktop/src/main`, `preload`, `shared` |

## 值得学习的机制

1. **core/views/ui/app 分层**
   - `core` 放无头业务逻辑、query key、API、stores。
   - `views` 放无 Next/Electron 依赖的业务页面。
   - `ui` 放纯 UI。
   - `apps/web` 和 `apps/desktop` 只做宿主适配。
   - AgentDash 后续 desktop 化时，可逐步把 `features` 中可复用部分拆成 views/core，而不是直接复制页面。

2. **desktop 是本机能力控制台**
   - multica desktop 有 daemon panel、settings tab、runtime card、daemon log parsing。
   - main 侧有 `daemon-manager.ts`、`cli-bootstrap.ts`、`runtime-config-loader.ts`、`updater.ts`。
   - AgentDash desktop 不应只是 Vite web wrapper，应负责 local backend 启停、健康、日志、版本、accessible roots、MCP server 状态。

3. **IPC bridge 隔离本机能力**
   - multica renderer 通过 platform/daemon IPC bridge 访问本机 daemon 状态。
   - AgentDash 可把 local backend 管理、日志 tail、配置写入、启动重启封装在 Tauri/Electron command 层，前端只消费类型化接口。

4. **server state 与 client state 分离**
   - multica 用 TanStack Query 承载 server state，Zustand 承载 tab/window/draft/filter。
   - AgentDash 当前 Zustand stores 兼容早期快速开发，但状态变多后容易让 server state 分散。

5. **desktop 独立 profile**
    - 原始 review 已指出 multica desktop 使用专属 profile 管 daemon，避免污染用户 CLI 手动 profile。
    - AgentDash local backend/desktop 应区分开发期 `pnpm dev`、用户手动 local、desktop 管理 local。

6. **token sync 与用户切换收敛**
   - `daemon-manager.ts` 会把登录态同步给 desktop-managed daemon，并在用户切换/登出时清 token、停止或重启 daemon。
   - AgentDash 后续 desktop 需要明确 web token、backend token、desktop-managed local profile 的边界，不能简单复用开发期 local 配置。

7. **version mismatch 安全重启**
   - multica 将版本决策抽成 `version-decision.ts`，daemon/CLI 与 desktop bundle 不匹配时可重启，但有 active task 时 defer。
   - AgentDash local backend 承担 session、terminal、MCP、VFS；安全重启判定应基于 active session、terminal、MCP call、materialization 等更完整的 active work 统计。

8. **本机即时状态桥接到 server state**
   - desktop renderer 的 daemon IPC bridge 会把本机 daemon 状态写入 runtimes query cache，形成“服务器权威状态 + 本机即时反馈”的双通道体验。
   - AgentDash desktop 可同样融合 cloud relay status 与 local command status，但 UI 必须标注来源和冲突优先级。

9. **日志 tail 与诊断 UI**
   - daemon panel 支持日志 tail、搜索、级别过滤、重复日志折叠、copy、clear。
   - AgentDash local backend 日志可能包含路径、token、prompt 或文件片段，正式设计前需要脱敏和限量策略。

## 不应直接照搬

- AgentDash 不一定要采用 Electron；如果 Tauri 方案已在规划，应借鉴职责边界而不是技术栈。
- 不要一次性重构全部前端到 monorepo packages；可以从 desktop 需要复用的 query/services/views 开始。
- Session stream、Hook Runtime、Context Inspector 不应被普通 query/invalidation 模式吞掉，需要保留专用流。
- Desktop 不应绕过 cloud/local 协议直接改数据库或工作区；所有状态变更仍应走 API/relay/local command。

## 后续正式任务候选

1. `feat(desktop): local backend daemon manager 与 health/log UI`
2. `refactor(frontend): 提取 server-state query layer 与 query key 规范`
3. `refactor(frontend): 抽取跨 web/desktop 可复用业务 views`
4. `feat(desktop): local profile/config 隔离与版本安全重启策略`
5. `feat(local): accessible roots/MCP/executors 可视化管理面板`
6. `feat(desktop): local profile/token/version 安全策略`
7. `feat(runtime): cloud relay status 与 local IPC status 融合展示`
