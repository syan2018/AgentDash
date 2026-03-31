# PR：收敛 Live Canvas Runtime 的稳定 mount 与展示闭环

## 关联背景

本 PR 对应 Live Canvas Runtime 的正式收口工作，目标是把 Canvas 从“已有基础组件”推进到“agent 可直接创建、写入并展示”的真实主链路。

关联 issue：[ #1 Live Canvas Runtime：打通 Agent 创建、编辑与展示 Canvas 的主链路](https://github.com/syan2018/AgentDash/issues/1)

## 本次改动

### 1. 收敛 Canvas 的稳定 agent-facing 标识

- Canvas 实体新增 `mount_id`
- `create_canvas` 支持传入稳定 `id`
- `create_canvas` 返回给 agent 的 `canvas_id` / `mount_id` 都直接使用稳定 `mount_id`
- Canvas mount 的实际挂载名直接使用 `mount_id`
- `inject_canvas_data` / `present_canvas` 支持通过稳定 id 或内部 UUID 解析 Canvas

### 2. 打通“创建后立刻写入”的同轮工具链

- 为 runtime FS 工具引入共享 `AddressSpace`
- `create_canvas` 成功后会把新 Canvas mount 追加到共享运行时 address space
- 同一轮后续的 `fs_write` / `fs_apply_patch` / `shell_exec` 等路径解析逻辑可以立即看到该 mount
- API 侧 schema 测试同步适配新的共享 address space 构造方式

### 3. 补齐持久化 / API / 前端对 `mount_id` 的统一感知

- SQLite `canvases` 表新增 `mount_id` 列与 `(project_id, mount_id)` 唯一索引
- 启动时若旧表缺列，自动补列并把旧数据回填为 `id`
- Canvas API DTO、前端类型与服务层补齐 `mount_id`
- 项目级 Canvas 管理视图展示实际 `mount_id`

### 4. 修复运行时预览与联调脚本

- Canvas 预览 iframe sandbox 调整为可正常加载运行时页面
- `pnpm dev` 对应的 [scripts/dev-joint.js](/F:/Projects/AgentDash/scripts/dev-joint.js) 在启动前默认清理残留 `agentdash-local`
- 避免重复 `backend_id` 注册导致联调环境被自动停掉

## 验证结果

- `cargo check -p agentdash-application -p agentdash-api -p agentdash-infrastructure -p agentdash-executor`
- `cargo test -p agentdash-application create_canvas_updates_shared_mounts -- --nocapture`
- `cargo test -p agentdash-api runtime_tool_schemas_are_openai_compatible -- --nocapture`
- `pnpm --dir frontend exec tsc --noEmit`
- 真实 UI 验收已跑通：
  `create_canvas(agent-mounted-kpi-v6)` → `fs_write(agent-mounted-kpi-v6://src/main.tsx)` → `present_canvas(agent-mounted-kpi-v6)` → Canvas 面板成功展示 KPI 页面

## 风险与后续

- 当前 `canvas_id` 入参仍兼容 UUID，是为了让已有内部路径解析更平滑；agent-facing 约定应统一优先使用稳定 `mount_id`
- 联调脚本清理的是进程名为 `agentdash-local` 的残留实例，如果未来本机后端 binary 改名，需要同步更新脚本
- 本 PR 不处理 merge，后续由完整 review 流程决定是否继续调整
