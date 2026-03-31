## 关联 Issue

Closes #1

## 背景

这组改动把 Live Canvas Runtime 从“规划中的概念”推进到一条可实际使用的主链路：Agent 可以创建项目级 Canvas 资产，随后直接通过统一 fs 工具维护文件，并在会话页把运行结果展示出来。

同时，这次也补了一处实际踩到的开发流问题：`pnpm dev` 以前只清端口，不清理残留 `agentdash-local`，旧进程会因为复用相同 `backend_id` 导致本机后端重复注册失败，进而把整套本地联调流程拉崩。

## 方案摘要

- Canvas 继续作为项目级独立资产实现，不复用 `context_containers`
- Agent-facing 标识统一收敛为稳定 `mount_id`，不再把内部 UUID 暴露给 Agent
- Canvas mount 直接加入 session address space，文件编辑继续复用既有 `fs_*` 工具
- 会话展示通过 `present_canvas` + runtime snapshot + 前端 Canvas 面板闭环打通
- 启动脚本新增残留 `agentdash-local` 清理，稳定本地联调

## 主要改动

### 后端 / 领域

- 为 Canvas 实体新增 `mount_id`，并扩展仓储查询与唯一约束
- SQLite 持久化补充 `mount_id` 列与旧数据回填逻辑
- `create_canvas` 支持显式传入稳定 `id`
- `present_canvas` / `inject_canvas_data` 支持通过稳定 `mount_id` 或 UUID 定位 Canvas
- Canvas mount 命名直接使用 `mount_id`
- 为 runtime tool 与 address space 追加逻辑补齐 Canvas 接入

### 前端

- 修正 `CanvasRuntimePreview` iframe sandbox，恢复预览执行能力
- 项目级 Canvas 管理视图增加 `mount_id` 展示
- Canvas API / 类型补充 `mount_id`

### 工程脚本

- `scripts/dev-joint.js` 启动前新增 `agentdash-local` 残留进程清理
- 保持 `--skip-local` 时不误杀用户想复用的本机后端

## 验证

- `cargo check -p agentdash-application -p agentdash-api -p agentdash-infrastructure -p agentdash-executor`
- `pnpm --dir frontend exec tsc --noEmit`
- `node --check scripts/dev-joint.js`
- 已完成真实会话链路验证：
  - 创建会话
  - Agent 调用 `create_canvas(id=...)`
  - Agent 对 `<mount_id>://src/main.tsx` 继续写入
  - Agent 调用 `present_canvas`
  - 前端打开并展示 Canvas

## 已知限制

- 当前预览模型仍是项目内受控 runtime，不提供通用 playground / 任意 npm 安装能力
- `present_canvas` / `inject_canvas_data` 仍兼容 UUID 引用，当前主要是为了平滑读取已有数据
- 这次没有处理后续完整 review / merge 流程之外的额外清理项

## 建议 Review 顺序

1. `mount_id` 对外语义与仓储持久化
2. session address space / runtime tool 的 Canvas 接入
3. 前端 Canvas 预览与项目级管理视图
4. `pnpm dev` 启动脚本的进程清理补丁
