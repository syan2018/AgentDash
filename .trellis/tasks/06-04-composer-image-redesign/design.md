# [child-2+3] 技术设计：Composer 重构 + 内联模型选择器 + 图片输入

## 布局目标

```
┌─────────────────────────────────────────────────────────┐
│ [FileReferenceTags 药丸区]                                │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ RichInput (多行文本区)                                │ │
│ │ 保留 @ 引用、slash 命令                               │ │
│ └─────────────────────────────────────────────────────┘ │
│ [图片预览区 - 缩略图行，单张可删除]                        │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ [+]  [Model Chip ⌄]                    [↑ / ■]     │ │  ← 底部工具栏
│ └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
  helperText
```

## 组件拆分

### 新增组件

1. **`InlineModelSelector.tsx`** — 紧凑 chip + 两级 popup
   - Props: execConfig + discovered + discovery (从 ExecutorSelector 提取所需子集)
   - Chip 文案: `<模型简称> <推理档>` 或 "选择模型…"
   - Popup: Popover（左列 Reasoning 档位 + provider 入口，右列模型列表）
   - `readonly` prop: Phase B steer 态只读预留

2. **`ComposerPlusMenu.tsx`** — 「+」菜单按钮
   - Popover 菜单，MVP 仅一行「添加文件/图片」
   - 文件 → 触发 fileRef.openPicker / 系统 file input
   - 图片 → 触发隐藏 file input(accept="image/*") → 走 useImageAttachments

3. **`ImageAttachmentPreview.tsx`** — 图片缩略图预览行
   - 展示 attachments 数组的缩略图 + 删除按钮
   - 空时不渲染

4. **`ComposerSendButton.tsx`** — morphing 发送按钮
   - 状态机: idle→↑发送, running+空→■停止, running+有输入→↑发送
   - `isRunning`, `hasInput`, `disabled` props
   - Phase B: 添加 `onQueue` / `onSteer` 回调

### 新增 Hook

5. **`useImageAttachments.ts`** — 图片附件管线状态管理
   - `attachments: ImageAttachment[]` (id, file, dataUrl, previewUrl)
   - `addFromFiles(files: FileList)` — 校验 mime/大小/数量，FileReader → data URL
   - `addFromClipboard(items: DataTransferItemList)` — 同上
   - `removeAttachment(id)` / `clearAll()`
   - 约束常量: MAX_IMAGE_SIZE=5MB, MAX_IMAGE_COUNT=6

### 改造组件

6. **`SessionChatComposer`** — 重构布局，集成上述新组件
   - 去掉 showExecutorSelector + ExecutorSelector 直接引用
   - 底部工具栏: [ComposerPlusMenu] [InlineModelSelector] ... [ComposerSendButton]
   - RichInput 添加 onPaste/onDrop 回调 → useImageAttachments
   - helperText 仍保留在底部

7. **`SessionChatView`** — onPrimaryAction 签名扩展
   - prompt 参数从 string → `{ text: string, attachments: ImageAttachment[] }`
   - handlePrimarySubmit 构建 `UserInput[]`: text + images as data URL

## 内联模型选择器设计

Chip 渲染逻辑:
- `executor` 为空 → "选择模型…"
- 有 `modelId` → 取 `modelInfo.name` 的简称 + thinkingLevel label
- 无 `modelId` 有 `executor` → executor name

Popup 结构 (Popover):
```
┌─ 左列 ────────────────────────┐
│ REASONING                     │
│ ○ 关闭                        │  (off)
│ ○ 最少                        │  (minimal)
│ ○ 低                          │  (low)
│ ○ 中                          │  (medium)
│ ● 高                          │  (high) ← 当前
│ ○ 超高                        │  (xhigh)
│ ─────────────────────────     │
│ PROVIDER                      │
│ Provider A                  ▸ │ → 展开右列
│ Provider B                  ▸ │
│ ─────────────────────────     │
│ [高级设置] [重置]              │
└───────────────────────────────┘
```

右列（悬停/点击 provider 展开）:
```
┌─ Provider A 模型 ─────────────┐
│ model-1                    ✓ │ ← 当前选中
│ model-2                      │
│ model-3                      │
└───────────────────────────────┘
```

## 发送按钮状态机

```
状态 = f(controlState, isRunning, inputHasContent)

idle + 有内容        → ↑ 发送 (primary action)
idle + 无内容        → ↑ 发送 (disabled)
running + 有内容     → ↑ 发送 (排队 - Phase B; 当前走 steer)
running + 无内容     → ■ 停止 (cancel)
```

Phase A 阶段: running+有内容 仍走当前 steer 逻辑；Phase B 扩展为 queue/steer 分流。

## 图片附件管线

```
粘贴(Ctrl+V) / 拖拽(onDrop) / picker(file input)
    ↓
useImageAttachments.addFromFiles/addFromClipboard
    ↓ 校验 mime type (image/*), 大小 (≤5MB), 数量 (≤6)
    ↓ FileReader.readAsDataURL → data URL
    ↓
attachments[] → ImageAttachmentPreview (缩略图)
    ↓ 发送时
构建 UserInput[]:
  - { type: "text", text, text_elements: [] }  (如有文本)
  - { type: "image", url: dataUrl }             (每张图)
  - + file references (Mention) from fileRef
```

## 实施步骤

S1. 新增 `useImageAttachments` hook + 单测
S2. 新增 `InlineModelSelector` 组件（chip + popup）
S3. 新增 `ComposerPlusMenu`、`ImageAttachmentPreview`、`ComposerSendButton`
S4. 重构 `SessionChatComposer` 布局，集成全部新组件
S5. 扩展 `SessionChatView` 的 submit 流程支持图片附件
S6. 更新 `SessionPage` 的 `handleAgentSessionPrimaryAction` 构建 UserInput[]
S7. 更新测试 + typecheck 验证
