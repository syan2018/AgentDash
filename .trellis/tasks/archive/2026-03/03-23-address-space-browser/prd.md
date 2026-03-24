# 统一 Address Space 浏览器与工作空间可视化

## Goal

建立一个统一的 Address Space 浏览器组件，在 Project / Story / Session 三个层级让用户直观查看当前可用的所有 mount 及其内容。同时优化工作空间创建和管理体验。

## 背景

当前系统存在"虚拟工作空间"概念，Agent 通过 Mount 挂载访问虚拟工作空间中的内容。但存在以下问题：

1. **工作空间内容完全不可视** — 用户无法浏览 workspace/mount 内的文件
2. **双轨文件 API 体验割裂** — 会话用 `/workspace-files`，Story 用 `/address-spaces`
3. **Address Space 信息藏在技术详情** — 普通用户难以发现
4. **Workspace 创建体验原始** — 手动输入绝对路径，无验证反馈

## Requirements

### Phase 0: 后端 API 增强

- [ ] 扩展 `/address-spaces/{space_id}/entries` 支持目录级浏览（path + recursive 参数）
- [ ] 扩展 `AddressEntry` 返回值增加 `is_dir`、`size` 字段
- [ ] 为 `inline_fs` 类型的 mount 增加条目检索支持
- [ ] 新增 mount 级文件读取 API：`POST /api/address-spaces/read-file`
- [ ] 新增 Address Space 预览 API：`POST /api/address-spaces/preview`

### Phase 1: 统一 Address Space 浏览器组件

- [ ] 扩展前端 `addressSpaces.ts` service 层
- [ ] 创建 `AddressSpaceBrowser` 核心组件（mount 选择 + 文件树 + 文件预览）
- [ ] 支持 panel / drawer / inline 三种布局变体

### Phase 2: 三个层级嵌入

- [ ] 项目设置页：workspace 详情中嵌入文件浏览 + address space 预览区
- [ ] Story 页：ContextPanel 中新增地址空间浏览区
- [ ] 会话页：重构 SharedFoldersSurfaceCard 为可交互式 mount 卡片

### Phase 3: 管理体验优化

- [ ] Workspace 创建流程优化（路径验证 + Git 预览）
- [ ] 默认 workspace 绑定引导
- [ ] 清理死代码（pickDirectory 等）

## Acceptance Criteria

- [ ] 用户可以在项目设置页预览 address space 的 mount 列表和文件内容
- [ ] 用户可以在 Story 页查看 story 级 address space 的所有 mount
- [ ] 用户可以在会话页查看当前会话实际可用的 address space 和文件
- [ ] 所有 mount 类型（relay_fs / inline_fs）都支持文件浏览
- [ ] relay_fs mount 显示 backend 在线状态
- [ ] inline_fs mount 支持直接查看内联文件内容
- [ ] Workspace 创建时能实时验证路径有效性并预览 Git 信息

## Technical Notes

### 数据流

```
Project 设置页 → POST /api/address-spaces/preview (project_id)
                 → build_derived_address_space(project, None, workspace, ...)
                 → ExecutionAddressSpace

Story 页       → POST /api/address-spaces/preview (project_id, story_id)
                 → build_derived_address_space(project, story, workspace, ...)
                 → ExecutionAddressSpace

会话页         → GET /api/{tasks|stories|projects}/{id}/session
                 → 已有的 session info 响应中的 address_space 字段
                 → ExecutionAddressSpace
```

### 关键约束

- 遵循 address-space-access.md 规范：统一 mount + relative path 定位模型
- relay_fs mount 的文件操作需要 backend 在线
- inline_fs mount 的文件数据在 mount.metadata.files 中
- 预览 API 不创建 session，仅推导 address space

### 涉及的后端文件

- `crates/agentdash-api/src/routes/address_spaces.rs` — API 扩展
- `crates/agentdash-api/src/routes.rs` — 路由注册
- `crates/agentdash-api/src/address_space_access.rs` — service 层

### 涉及的前端文件

- `frontend/src/services/addressSpaces.ts` — service 扩展
- `frontend/src/features/address-space/` — 新建浏览器组件目录
- `frontend/src/features/project/project-selector.tsx` — 嵌入
- `frontend/src/pages/StoryPage.tsx` — 嵌入
- `frontend/src/pages/SessionPage.tsx` — 重构
- `frontend/src/features/workspace/workspace-list.tsx` — 创建优化
- `frontend/src/stores/workspaceStore.ts` — 清理
