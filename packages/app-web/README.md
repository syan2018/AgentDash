# AgentDash 前端

基于 React + TypeScript + Vite 构建。

## 技术栈

- React 19
- TypeScript
- Tailwind CSS v4
- Zustand (状态管理)
- @dnd-kit (拖拽功能)

## 开发命令

> **注意**：本项目使用 `pnpm` 作为包管理器，不要使用 `npm`

```bash
# 安装依赖
pnpm install

# 启动开发服务器
pnpm dev

# 构建生产版本
pnpm build

# 运行 ESLint 检查
pnpm lint

# 运行 TypeScript 类型检查
pnpm exec tsc -b
```

## 项目结构

```
src/
├── components/      # 通用 UI 组件
├── features/        # 功能模块
│   ├── story/       # Story 看板、卡片、详情抽屉
│   ├── task/        # Task 卡片、详情抽屉
│   └── workspace/   # Workspace 管理
├── pages/           # 页面组件
├── stores/          # Zustand 状态管理
├── types/           # TypeScript 类型定义
└── api/             # API 客户端
```

## 核心功能

- **Story 看板**: 拖拽式看板视图，支持状态流转
- **实时状态**: 通过 SSE 接收后端状态变更推送
- **多项目管理**: Project → Workspace → Story → Task 层级

## 开发规范

开发前请阅读 `.trellis/spec/frontend/` 下的规范文档。
