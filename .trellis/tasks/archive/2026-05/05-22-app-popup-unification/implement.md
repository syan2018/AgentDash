# 统一应用内弹窗实践 - Implementation Plan

## Checklist

1. 读取编码前规范
   - `.trellis/spec/frontend/component-guidelines.md`
   - `.trellis/spec/frontend/design-language.md`
   - `.trellis/spec/guides/code-reuse-thinking-guide.md`
2. 在 `@agentdash/ui` 新增通用 dialog primitive
   - 新增 `ConfirmDialog`。
   - 新增 `PromptDialog`。
   - 从 `packages/ui/src/index.ts` 导出组件和 props 类型。
   - 在 `DesignSystemPage` 增加示例，验证普通确认、危险确认、输入弹窗。
3. 迁移 `VfsBrowserPanel`
   - 用状态驱动新建 / 重命名 / 上传路径输入弹窗。
   - 用状态驱动删除确认弹窗。
   - 保留当前权限、路径规范化、树刷新、选中文件、图片预览和错误显示逻辑。
4. 迁移 `SkillExtraFilesEditor`
   - 新建 / 重命名使用 `PromptDialog`。
   - 删除使用 `ConfirmDialog tone="danger"`。
   - 保留 `normalizeSkillExtraPath`、重复路径判断和选中路径更新。
5. 迁移 Settings Provider 删除
   - 用 `ConfirmDialog tone="danger"` 替换 inline `window.confirm`。
   - 保持现有 `onDelete` 调用边界。
6. 迁移 Workflow 未保存离开确认
   - 用 pending navigate 状态或专用 open state 替换同步 confirm。
   - 确认后执行现有 `navigate("/dashboard/assets/workflow")`。
7. 迁移 Transition kind 切换确认
   - artifact -> flow 且存在 bindings 时记录 pending kind。
   - 确认后执行 `onSetKind(pendingKind)`。
8. 调整 Codex OAuth 外部授权打开
   - 移除 `window.open("about:blank")` 预开窗口。
   - 移除自动 `window.open(flow.auth_url, ...)` fallback。
   - 保留桌面 `openDesktopExternalBrowser(auth_url)`。
   - Web 环境显示应用内授权入口，由用户显式打开授权链接。
9. 清扫浏览器弹窗依赖
   - `rg -n "window\\.prompt|window\\.confirm|window\\.alert|\\bprompt\\(|\\bconfirm\\(|\\balert\\(" packages/app-web/src packages/ui/src`
   - `rg -n "window\\.open" packages/app-web/src`
   - 对仍保留的 `window.open` 必须确认是用户显式动作或改为普通链接。
10. 验证
   - `pnpm --filter @agentdash/ui typecheck`
   - `pnpm --filter app-web exec tsc --noEmit -p tsconfig.app.json`
   - `pnpm --filter app-web lint`
   - `pnpm --filter app-web test -- vfs-browser-panel`
   - 如改动测试命令不支持筛选，则运行 `pnpm --filter app-web test`

## Risky Files

- `packages/app-web/src/features/vfs/vfs-browser-panel.tsx`：文件操作状态较多，需避免引入 stale closure。
- `packages/app-web/src/features/assets-panel/categories/SkillCategoryPanel.tsx`：组件较大，迁移时只触碰附加文件编辑区域。
- `packages/app-web/src/pages/SettingsPage.tsx`：OAuth flow 状态已有轮询和取消逻辑，不能破坏 `pollCodexOAuth` / `cancelCodexOAuth`。
- `packages/app-web/src/pages/LifecycleEditorShellPage.tsx`：返回导航从同步 confirm 改为异步 dialog，需要避免重复导航。
- `packages/app-web/src/features/workflow/ui/transition-inspector.tsx`：select value 和 pending kind 要保持受控组件一致性。
- `packages/ui/src/primitives/*`：primitive API 会被后续业务复用，props 命名要稳定。

## Review Gate Before Start

用户已确认“统一完整迁移”。开始实现前需要确认本规划是否按全量范围执行：全部 `prompt` / `confirm` 迁移，并把 Codex OAuth 的自动 `window.open` fallback 改为应用内状态和显式授权入口。
