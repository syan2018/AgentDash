# VFS Markdown 资产预览支持

## Goal

让 VFS / 资产编辑区在继续使用 CodeMirror 编辑 Markdown 源码的同时，提供与前端会话展示一致的 Markdown 渲染预览，降低阅读 `.md` / `.mdx` / Skill 文档时的认知成本。

## Requirements

* 抽出可复用的前端 Markdown 渲染组件，复用现有 `streamdown` 插件能力。
* VFS CodeMirror 编辑器在 Markdown 文件中提供 `编辑` / `预览` / `分栏` 三种视图模式。
* Markdown 预览实时消费当前编辑草稿内容，未保存修改也能立即预览。
* 带 YAML frontmatter 的 Markdown 预览默认隐藏 frontmatter，只渲染正文内容。
* 非 Markdown 文件保持现有编辑器行为，不显示 Markdown 视图切换。
* 保存逻辑继续以 CodeMirror 当前内容为准，不改变后端 VFS API 契约。

## Acceptance Criteria

* [ ] `.md` / `.mdx` 文件打开后可切换编辑、预览、分栏。
* [ ] 预览支持现有会话消息已支持的代码块、数学公式、Mermaid、CJK 友好渲染。
* [ ] 修改 Markdown 内容后，预览同步更新，保存按钮脏状态仍正确。
* [ ] 普通代码文件行为不受影响。
* [ ] 前端类型检查与测试通过。

## Definition of Done

* 前端代码符合 FSD 目录与组件规范。
* 无新增 `any`、非空断言或调试日志。
* 新增/复用样式不破坏现有会话消息 Markdown 展示。
* 运行前端质量检查，记录结果。

## Technical Approach

* 将 `SessionMessageCard` 内部的 Markdown 渲染逻辑抽为共享组件，供会话消息与 VFS 预览共同使用。
* 在 `VfsCodeEditor` 内维护 `draftContent` 与视图模式状态；CodeMirror 文档变更时同步草稿。
* 对 Markdown 文件默认使用预览视图；非 Markdown 文件只渲染原编辑器。
* 使用现有 Tailwind 与 CSS 变量，保持工具型界面密度，不引入新的 Markdown 编辑器依赖。

## Decision (ADR-lite)

**Context**: 项目已经使用 CodeMirror 管理文件源码编辑，并在会话消息中使用 Streamdown 渲染 Markdown。

**Decision**: 保留 CodeMirror 源码编辑体验，抽出 Streamdown 渲染器并在 VFS 编辑器中加入 Markdown 预览模式。

**Consequences**: 实现成本低、展示一致性好；暂不解决滚动同步和所见即所得编辑，后续可以按需要渐进增强。

## Out of Scope

* 所见即所得 Markdown 编辑。
* 编辑器与预览滚动位置同步。
* 服务端 Markdown 渲染。
* 新增 Markdown AST 解析或转换管线。
* 独立处理 Skill frontmatter 的富展示。

## Technical Notes

* 现有 CodeMirror 编辑器位于 `frontend/src/features/vfs/vfs-code-editor.tsx`。
* 现有 Markdown 渲染逻辑位于 `frontend/src/features/session/ui/SessionMessageCard.tsx`。
* 前端规范参考 `.trellis/spec/frontend/index.md`、组件规范、目录结构、类型安全与质量规范。
