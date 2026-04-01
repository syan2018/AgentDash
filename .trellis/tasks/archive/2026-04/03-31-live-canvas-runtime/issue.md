# Live Canvas Runtime：打通 Agent 创建、编辑与展示 Canvas 的主链路

## 背景

当前 AgentDash 已经有基础的 Canvas 资产、runtime snapshot API 和 Session 侧预览面板，但距离“面向生产环境低代码同学可直接使用”的目标，还差最后一段关键闭环：

- Agent 需要能自己创建 Canvas，而不是依赖人手工先建好
- `create_canvas` 后，后续同一轮里的 `fs_write` 必须立刻能写到新 mount 上
- Agent-facing 的 Canvas 标识不能暴露内部 UUID，而应使用稳定、可读、可推断的 mount id
- Session 页需要能在 agent 发出 `present_canvas` 后自动打开并展示结果
- 本地开发联调需要更稳定，避免残留 `agentdash-local` 进程把整套 `pnpm dev` 拉崩

## 目标

把 Live Canvas 收口成一条真实可用的主链路：

1. Agent 调用 `create_canvas(id=...)` 创建 Canvas 与 mount
2. Agent 直接通过 `fs_write(<stable-mount-id>://...)` 写入文件
3. Agent 调用 `present_canvas(canvas_id=<stable-mount-id>)` 请求展示
4. Session 页自动打开 Canvas 面板并渲染运行时内容
5. 项目级 Canvas 管理视图、API 与运行时 mount 命名统一使用稳定 `mount_id`

## 范围

### 后端 / 应用层

- 引入共享运行时 `AddressSpace`，让 `create_canvas` 后追加的 mount 对同轮后续 `fs_*` 工具立即可见
- 将 Canvas 的 agent-facing 标识收敛为 `mount_id`
- `create_canvas` 支持可选稳定 id，并在返回值中直接暴露该稳定 id
- `inject_canvas_data` / `present_canvas` 支持通过稳定 id 解析 Canvas
- Canvas mount 的实际挂载名直接使用 `mount_id`

### 持久化 / API

- Canvas 仓储新增 `mount_id` 字段、查询接口与 `(project_id, mount_id)` 唯一约束
- 兼容已有本地 SQLite 数据：若旧表没有 `mount_id`，启动时自动补列并回填
- API DTO 与前端类型补齐 `mount_id`

### 前端

- 运行时 iframe 预览修复为可正常加载实际运行页面
- 项目级 Canvas 列表与详情显式展示 `mount_id`
- Session 页保留 `canvas_presented` 主链路，支持真实打开预览

### 工程脚本

- `pnpm dev` 的联合启动脚本默认在启动前清理残留 `agentdash-local`，降低重复 `backend_id` 注册导致的联调失败率

## 验收标准

- [ ] Agent 可以只通过工具调用完成 Canvas 创建、文件写入与展示，不依赖手工建 Canvas
- [ ] `create_canvas` 返回的 `canvas_id` / `mount_id` 对 agent 可直接使用，且不暴露内部 UUID
- [ ] 同一轮会话中，`create_canvas` 后紧跟的 `fs_write(<mount_id>://src/main.tsx)` 能成功
- [ ] Session 页能在 `present_canvas` 后打开 Canvas 面板并成功预览
- [ ] 项目级 Canvas 管理视图与 API 返回中都能看到稳定 `mount_id`
- [ ] `pnpm dev` 重启时不会再因为残留 `agentdash-local` 导致本机后端重复注册

## 验证

- `cargo check -p agentdash-application -p agentdash-api -p agentdash-infrastructure -p agentdash-executor`
- `cargo test -p agentdash-application create_canvas_updates_shared_mounts -- --nocapture`
- `cargo test -p agentdash-api runtime_tool_schemas_are_openai_compatible -- --nocapture`
- `pnpm --dir frontend exec tsc --noEmit`
- 已完成一次真实 UI 链路验收：
  `create_canvas(agent-mounted-kpi-v6)` → `fs_write(agent-mounted-kpi-v6://src/main.tsx)` → `present_canvas(agent-mounted-kpi-v6)` → Session 右侧 Canvas 面板成功显示 KPI 页面

## 非目标

- 不处理 merge / 发布流程
- 不补充兼容性迁移方案，保持预研阶段的最正确状态
- 不继续扩展 PiAgent 底层工具执行策略
