# 统一应用内弹窗实践 - Design

## Scope

完整迁移范围覆盖本次扫描到的所有浏览器弹窗依赖：

- 输入弹窗：VFS Browser 文件路径输入、Skill 附加文件路径输入。
- 确认弹窗：VFS 删除、Skill 附加文件删除、Settings Provider 删除、Workflow 未保存离开、Transition kind 切换清空 bindings。
- 外部授权打开：Settings Codex OAuth 的 `window.open` 自动弹窗依赖。

`alert` 当前没有业务调用，因此不需要迁移现有入口，但质量检查会继续扫描。

## UI Primitive Boundary

在 `@agentdash/ui` 增加通用弹窗 primitive：

- `PromptDialog`
  - props：`open`、`title`、`description?`、`label`、`value`、`placeholder?`、`confirmLabel`、`cancelLabel?`、`disabled?`、`error?`、`onValueChange`、`onClose`、`onConfirm`。
  - 行为：打开时聚焦输入框；Enter 触发确认；Escape / overlay / 取消关闭；确认按钮由调用方根据值合法性控制。
- `ConfirmDialog`
  - props：`open`、`title`、`description`、`confirmLabel`、`cancelLabel?`、`tone?`、`disabled?`、`onClose`、`onConfirm`。
  - 行为：应用内 overlay；Escape / overlay / 取消关闭；调用方决定确认动作和关闭时机。
- `DangerConfirmDialog`
  - 保留已有输入匹配能力。
  - 可以继续服务 Project / Story / Task 等高风险删除。
  - 本次删除类轻量场景可以使用 `ConfirmDialog tone="danger"`，除非业务需要输入匹配。

这两个 primitive 放在 `packages/ui/src/primitives/` 并通过 `packages/ui/src/index.ts` 导出。若实现发现二者大部分布局重复，可抽文件内私有 shell，但对外 API 保持清晰。

## Data Flow

### VFS Browser

当前 VFS 文件操作是按钮点击后同步调用浏览器 prompt，再进入异步 service 调用。迁移后改为两段状态机：

1. 点击按钮设置 `pendingPrompt` / `pendingConfirm` 状态，弹出应用内 dialog。
2. 用户确认后沿用现有 `resolveScopedPath`、`createSurfaceFile`、`renameSurfaceFile`、`uploadSurfaceFileBlob`、`deleteSurfaceFile` 流程。
3. 成功后关闭 dialog、刷新树、同步选中文件和内容。
4. 失败后复用现有 `operationError` 展示；弹窗关闭，错误显示在面板顶部，保持与现有行为一致。

### Skill 附加文件

`SkillExtraFilesEditor` 当前在组件内直接 prompt / confirm。迁移后：

1. 新建和重命名按钮只打开 `PromptDialog`。
2. 确认时继续使用 `normalizeSkillExtraPath` 和重复路径检查。
3. 删除使用 `ConfirmDialog tone="danger"`。
4. 成功后保持现有 `files` 更新和 `selectedPath` 切换逻辑。

### Settings Provider 删除

Provider 删除属于设置页局部危险操作，迁移为 `ConfirmDialog tone="danger"`。确认后调用现有 `onDelete`，不改变 Provider 保存、探测、模型管理流程。

### Workflow 未保存离开

当前 `handleBack` 需要同步 confirm 决策。迁移后：

1. 若 `isDirty` 为 false，直接 navigate。
2. 若 `isDirty` 为 true，打开 `ConfirmDialog`。
3. 用户确认后 navigate 到 `/dashboard/assets/workflow`。

### Transition kind 切换

当前 select change 时同步 confirm。迁移后：

1. 触发 artifact -> flow 且存在 bindings 时，记录 `pendingKind = "flow"` 并打开 `ConfirmDialog`。
2. 用户确认后执行 `onSetKind(pendingKind)`。
3. 其它切换保持立即执行。

### Codex OAuth

授权流程仍需要外部身份提供方页面，因此不把授权页做成应用内 iframe。迁移目标是避免自动浏览器弹窗：

1. `handleStartCodexLogin` 只负责创建 flow、更新应用内状态、记录 `auth_url`。
2. 桌面环境可在 flow 创建成功后调用 `openDesktopExternalBrowser(auth_url)`，因为这是受控桌面能力，不是浏览器 popup fallback。
3. Web 环境显示应用内授权入口，让用户明确点击打开授权页。
4. 不再预开 `about:blank`，不再在 fallback 中自动 `window.open`。

## Compatibility And Migration

项目处于预研期，不保留原生 `prompt` / `confirm` fallback。替换后源码中不应继续出现 `window.prompt` / `window.confirm` / `window.alert` 业务调用。

`window.open` 不作为通用业务弹窗能力使用。外部 OAuth 授权可保留用户点击的普通链接或受控 desktop opener，但不保留自动 popup fallback。

## Trade-offs

- 通用 primitive 比业务内联 dialog 多一点初始成本，但能让文件操作、设置确认、Workflow 确认和后续同类交互走一致模型。
- Workflow 和 Transition 原本依赖同步 `confirm`，迁移后需要显式 pending 状态；这是更清晰的数据流，也能避免浏览器阻塞式 API 对 React 状态的隐性耦合。
- OAuth 不再自动打开浏览器窗口后，Web 环境会多一次用户点击；换来的是不依赖 popup blocker，状态也更可解释。
