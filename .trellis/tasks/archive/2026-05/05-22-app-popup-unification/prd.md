# 统一应用内弹窗实践

## Goal

将前端中依赖浏览器原生弹窗或浏览器新窗口承载的关键交互，完整收敛到 AgentDashboard 自己的应用内 popup / dialog 体系。目标是让用户确认、路径输入、危险操作、流程状态提示都由应用自身控制，避免部分浏览器环境不支持或拦截原生弹窗导致功能不可用。

## Requirements

- 全量迁移前端业务代码里的浏览器原生输入和确认弹窗。
  - `window.prompt` 必须迁移为应用内输入弹窗。
  - `window.confirm` 必须迁移为应用内确认弹窗。
  - `window.alert` 当前未命中；后续若出现也应使用应用内提示或 dialog。
- VFS Browser 文件操作不得依赖浏览器原生弹窗。
  - 新建文件需要应用内输入弹窗，保留当前建议路径能力。
  - 重命名文件需要应用内输入弹窗，保留当前路径预填能力。
  - 上传图片后的保存路径需要应用内输入弹窗，保留默认 `assets/<filename>` 建议。
  - 删除文件需要应用内确认弹窗，保留当前受保护文件、权限和 busy 状态约束。
- Skill 附加文件编辑器需要同步迁移新建、重命名、删除确认，作为同类文件操作的复用落点。
- Settings Provider 删除、Workflow 未保存离开确认、Transition artifact -> flow 清空绑定确认，都需要迁移为应用内确认弹窗。
- Settings Codex OAuth 不应再依赖浏览器弹出空白窗口或自动新窗口 fallback。
  - 授权页面仍是外部身份授权流程，不能被简单塞进应用内 iframe。
  - 应用内需要承载登录流程状态、授权链接展示和明确的“打开授权页”操作。
  - 桌面环境继续优先使用 `openDesktopExternalBrowser`。
  - Web 环境不再自动 `window.open`，改为由用户显式点击链接或按钮打开授权页。
- 应用内弹窗应复用或补齐 `@agentdash/ui` primitive，而不是在业务组件内重复堆叠遮罩、按钮、输入框和确认布局。
- 普通确认、危险确认、输入确认需要清晰区分：
  - 普通确认用于未保存离开、清空 bindings 等可撤销或局部影响操作。
  - 危险确认用于删除类操作；高风险删除继续支持输入匹配。
  - 输入确认用于路径、名称等用户输入。
- 弹窗交互需要支持键盘和焦点：
  - 打开输入弹窗时自动聚焦输入框。
  - Enter 确认，Escape 或取消按钮关闭。
  - 确认中/操作中禁止重复提交。

## Confirmed Evidence

- `packages/app-web/src/features/vfs/vfs-browser-panel.tsx`
  - `handleCreateFile` 使用 `window.prompt("新建文件路径", suggestedPath)`。
  - `handleRenameFile` 使用 `window.prompt("重命名为", currentPath)`。
  - `handleUploadImage` 使用 `window.prompt("图片保存路径", suggestedPath)`。
  - `handleDeleteFile` 使用 `window.confirm(...)`。
- `packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx`
  - 附加文件的新建、重命名、删除分别使用 `window.prompt` / `window.confirm`，与 VFS Browser 属于同类文件操作交互。
- `packages/app-web/src/pages/SettingsPage.tsx`
  - 删除 Provider 使用 `window.confirm`。
  - Codex OAuth 使用 `window.open("about:blank", "_blank")` 预开窗口，并在 fallback 中使用 `window.open(flow.auth_url, "_blank", "noopener,noreferrer")`。
- `packages/app-web/src/pages/LifecycleEditorShellPage.tsx`
  - 离开未保存 Workflow 时使用 `window.confirm`。
- `packages/app-web/src/features/workflow/ui/transition-inspector.tsx`
  - artifact -> flow 切换会通过 `window.confirm` 提醒清空 bindings。
- `rg` 未发现 `window.alert` / `alert(` 业务调用。
- 前端设计规范要求优先使用 `@agentdash/ui` primitive；同一形态在两个以上文件出现就抽进 primitive。

## Acceptance Criteria

- [ ] `packages/app-web/src` 中不再出现 `window.prompt`、`window.confirm`、`window.alert` 业务调用。
- [ ] VFS Browser 新建文件、重命名文件、上传图片保存路径、删除文件全程使用应用内弹窗，且建议路径、路径规范化、权限禁用、受保护文件禁用、错误展示、刷新文件树和选中文件行为保持一致。
- [ ] Skill 附加文件的新建、重命名、删除确认使用同一套应用内弹窗 primitive。
- [ ] Settings Provider 删除、Workflow 未保存离开确认、Transition 清空 bindings 确认使用应用内确认弹窗。
- [ ] Codex OAuth 不再自动依赖 `window.open` 弹出窗口；应用内承载登录状态和授权入口，桌面环境仍通过 desktop external browser opener 打开外部授权页。
- [ ] 新增或复用的应用内弹窗 primitive 在 `@agentdash/ui` 中具名导出，并符合当前设计语言 token / radius / surface 约束。
- [ ] 前端 typecheck、lint、相关测试通过。

## Notes

- 本任务处于预研期项目，不需要为旧浏览器或旧交互保留兼容 fallback；应直接把项目推进到正确实践。
