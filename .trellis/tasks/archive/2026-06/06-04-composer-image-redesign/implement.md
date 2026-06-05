# [child-2+3] 执行计划

校验命令：
- 前端：`pnpm -F app-web typecheck` + `pnpm -F app-web lint` + `pnpm -F app-web test`

## 步骤

### S1 useImageAttachments hook
- [ ] 新建 `packages/app-web/src/features/session/ui/composer/useImageAttachments.ts`
- [ ] 单测覆盖 mime 校验、大小限制、数量限制、data URL 生成

### S2 InlineModelSelector 组件
- [ ] 新建 `packages/app-web/src/features/executor-selector/ui/InlineModelSelector.tsx`
- [ ] Chip + Popover 两级菜单（reasoning + provider/model）
- [ ] readonly prop 预留

### S3 辅助组件
- [ ] `ComposerPlusMenu.tsx` — 「+」菜单
- [ ] `ImageAttachmentPreview.tsx` — 图片缩略图
- [ ] `ComposerSendButton.tsx` — morphing 发送按钮

### S4 SessionChatComposer 布局重构
- [ ] 底部工具栏布局
- [ ] 集成 InlineModelSelector / ComposerPlusMenu / ComposerSendButton
- [ ] RichInput 添加 paste/drop 图片支持
- [ ] 移除对旧 ExecutorSelector 的直接引用

### S5 Submit 流程扩展
- [ ] handlePrimarySubmit 构建 `UserInput[]`（含图片 data URL）
- [ ] onPrimaryAction 签名更新

### S6 SessionPage 适配
- [ ] handleAgentSessionPrimaryAction 支持图片 UserInput

### S7 测试 + 验证
- [ ] 更新 SessionChatView.test.tsx
- [ ] typecheck + lint 通过
