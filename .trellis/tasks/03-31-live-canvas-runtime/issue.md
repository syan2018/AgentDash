# Live Canvas Runtime：打通 Agent 可创建的项目级 Canvas 资产与会话展示链路

## 背景

当前 AgentDash 的 Agent 产出主要以文本消息和结构化事件呈现，缺少一条让 Agent 在项目内创建、持续编辑、并直接向用户展示可运行前端资产的主链路。对于低代码同学的可视化面板、统计视图、交互式信息展示场景，这会带来三个明显问题：

- Agent 无法把 UI 结果沉淀为项目级资产，只能临时输出代码片段
- 会话侧缺少一条“生成代码后立即展示”的标准交互闭环
- 运行时文件与业务数据之间没有稳定的绑定/挂载模型，难以复用统一 address space / fs 工具

## 目标

建立一条以 Canvas 为一等资产的 Live Canvas Runtime 主链路：

- Agent 可通过 `create_canvas` 创建项目级 Canvas
- Canvas 以 mount 的形式加入会话 address space，后续文件编辑继续复用 `fs_*`
- Agent 可通过 `present_canvas` 触发前端打开并展示 Canvas
- 前端可基于独立 runtime snapshot 拉取并渲染 Canvas 内容
- Agent 只看到稳定 `mount_id`，不暴露内部 UUID

## 方案概要

### 1. Canvas 作为独立项目资产

- 新增 Canvas 领域实体与仓储能力
- Canvas 内部保留数据库主键 UUID，但额外引入 `mount_id` 作为 agent-facing 稳定标识
- `(project_id, mount_id)` 保持唯一，旧数据自动回填默认 `mount_id`

### 2. 运行时复用统一 Address Space / FS 工具

- 新增 `canvas_fs` provider 并加入 session address space
- Canvas 文件继续通过 `fs_read` / `fs_write` / `fs_apply_patch` / `fs_list` 等现有工具读写
- `create_canvas` 只负责创建实体与 mount；文件编辑不新增冗余专用工具

### 3. 会话展示与运行时预览闭环

- `present_canvas` / `inject_canvas_data` 支持通过稳定 `canvas_id` / `mount_id` 访问 Canvas
- 会话页消费 `canvas_presented` 事件并打开 Canvas 面板
- 前端 runtime preview 修正 iframe sandbox，确保预览能正常执行脚本
- Canvas 页面与 workflow 同级，作为项目级一等入口

### 4. 开发流稳定性补丁

- `pnpm dev` 启动前除清理端口外，也应清理残留 `agentdash-local`
- 避免旧本机后端占用相同 `backend_id` 导致重复注册失败并拖垮整套 dev 服务

## 本次范围

- Canvas 领域实体、持久化、API 与前端类型
- Session address space 中的 Canvas mount 追加
- `create_canvas` / `present_canvas` / `inject_canvas_data` 主链路
- Canvas 预览面板的运行时修正
- 项目级 Canvas 管理视图的 `mount_id` 展示
- `pnpm dev` 启动前清理残留 `agentdash-local`

## 验收标准

- [ ] Agent 可在会话中通过 `create_canvas(id=...)` 创建 Canvas
- [ ] 后续 `fs_write(<mount_id>://src/main.tsx, ...)` 可直接写入该 Canvas mount
- [ ] `present_canvas(<mount_id>)` 可在会话页打开并展示对应 Canvas
- [ ] Agent 侧不再看到内部 UUID 形式的 mount 名称
- [ ] 项目级 Canvas 列表可显示稳定 `mount_id`
- [ ] `pnpm dev` 在存在残留 `agentdash-local` 时仍能稳定启动，不再因重复 backend 注册而中断

## 验证记录

- `cargo check -p agentdash-application -p agentdash-api -p agentdash-infrastructure -p agentdash-executor`
- `pnpm --dir frontend exec tsc --noEmit`
- 真实前端交互链路已验证通过：创建会话、由 Agent 调用 `create_canvas` 创建 mount、Agent 继续编辑 Canvas、最终展示信息

## 已知限制

- 当前 runtime 仍以受控预览和项目内资产管理为主，不追求通用在线 IDE 能力
- `present_canvas` / `inject_canvas_data` 目前兼容稳定 `mount_id` 与 UUID 引用，后续可视情况进一步收敛
- `pnpm dev` 当前只补了 `agentdash-local` 残留清理；若后续发现其他二进制残留问题，可按同一模式补充

## Review 关注点

- `mount_id` 作为 agent-facing 标识是否已经覆盖所有对外暴露点
- session address space 的共享与追加逻辑是否足够稳定
- runtime preview 的 iframe sandbox 调整是否符合当前预览模型
- dev 启动脚本的进程清理策略是否满足本地开发预期
