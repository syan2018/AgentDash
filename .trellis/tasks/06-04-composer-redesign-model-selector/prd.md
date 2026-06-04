# [child-3] Composer 重构与内联模型选择器

Parent: [06-04-session-composer-redesign-image-input](../06-04-session-composer-redesign-image-input/prd.md)
依赖：建议在 child-1 之后（发送字段已统一）；与 child-2（附件预览槽）、child-4（steer 态选择器只读）UI 协同。

## Goal

对 `SessionChatComposer` 做较大幅度重构为现代一体化布局，并把执行器/模型选择收进**内联的紧凑「模型 + 推理档」下拉**（参考截图：底部 chip 显示如「5.5 超高」，点开是 推理档 + 模型 两级菜单）。

## Requirements

- R1 布局重构：发送/取消从右侧竖排 → 底部一体化工具栏右对齐；输入区为主体；附件预览区（child-2）+ 文件引用药丸区协调排布。
- R2 内联模型/推理选择器：把现有 `ExecutorSelector` 收敛为工具栏内的紧凑入口（chip 展示当前 模型+推理档；popup 两级：推理档 + **按 provider 分组**的模型列表）。**截图仅参考样式**，实际推理档/模型以项目真实 discovered options 为准、按 `provider_id` 分组，选择同时落 `providerId`+`modelId`，不得硬编码截图型号。保留可发现模型/provider/permission/reset/reconnect 能力，仅改变呈现密度与位置。
- R2b 发送按钮形态（用户指定，与 child-4 协同）：单上箭头发送按钮，**无麦克风**；running+输入框空 → ■ 停止（cancel）；running+有新输入 → ↑ 发送；idle → ↑ 发送。取代现有"发送/取消"竖排双按钮。键盘语义（Enter 排队 / Ctrl+Enter 强制 / Shift+Enter 换行）权威定义在 child-4。
- R2c 「+」通用菜单栏：composer 左侧「+」= 会话快捷操作的通用 GUI 菜单（可扩展）。MVP 仅含一行「拉起引用本地文件/图片的选择器」（文件/图片 picker）：选文件→既有 `@` 引用（Mention）；选图片→喂 child-2 图片附件管线。**不是**模型选择器（模型/推理是左下独立 chip）。其余快捷项后续补。
- R3 能力保留：@ 文件引用、token 用量、helperText、promptTemplates、initialInputValue、disabled 态、steer/send/start_draft/cancel 多态主操作，全部不回归。
- R4 测试：更新 `SessionChatView.test.tsx` 适配新结构。

## Acceptance Criteria

- [ ] composer 为较大改版的一体化工具栏布局，含内联模型/推理选择器（截图风格）。
- [ ] 现有全部能力（@/token/helper/模板/多态主操作/取消）保留且测试通过。
- [ ] 选择器只改 `executor_config`，与输入表示正交；为 child-4 预留"steer 态选择器只读/隐藏"接口。
- [ ] `pnpm -F app-web lint/typecheck/test` 通过。

## 范围外

- 图片采集逻辑（child-2）；双模 action 模型（child-4）。
- 语音/麦克风输入（参考图里的麦克风图标忽略，不实现）。

## Notes

- 参考 UI（用户截图转录 + 待补原图）：[assets/reference-ui.md](assets/reference-ui.md)。
- 现有 `ExecutorSelector` 在 [executor-selector](../../../packages/app-web/src/features/executor-selector) 与 [SessionChatViewParts.tsx](../../../packages/app-web/src/features/session/ui/SessionChatViewParts.tsx)；design.md 在激活时给紧凑下拉的组件方案与复用边界。
