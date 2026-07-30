# Canvas 展示质量

## 交付检查

- 让主要用户任务可以立即操作。
- 保持标题和解释文案足够简短，适配 Workspace panel。
- 明确呈现 loading、empty、error、disabled 与 revision-conflict 状态。
- 需要共享或供 Agent 检查时，把有意义的选择与表单值保存在 canonical Interaction state。
- 暴露显式 command，不从 DOM 结构推断意图。
- 通过 `workspace_module_present` 展示 Canvas，并使用返回的 canonical URI。

## 视觉原则

- 优先构建聚焦的工具、dashboard、diagram、form 或 explorer，而不是通用 landing page。
- 优先使用层级、间距和排版，再考虑装饰性容器。
- 避免 card 套 card，以及没有信息价值的装饰渐变。
- 保证控件在常见 Workspace panel 宽度下可操作。
- 使用语义化 label、键盘可操作控件和足够的对比度。
- 使用合适的 table、plot、diagram 或渐进披露呈现高密度数据。

## 最终验证

确认最新 descriptor 仍暴露所选 view，presentation 打开目标 definition 或 instance，并保证 UI
在没有隐藏 Agent-only context 时仍然可用。
