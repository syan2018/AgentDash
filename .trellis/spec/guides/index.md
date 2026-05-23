# Thinking Guides

Guides 是 thinking harness：它们提醒开发前要检查哪些问题，但不定义架构权威契约。具体不变量与执行契约应回到相关 `architecture.md` 或 appendix。

## Available Guides

| Guide | Purpose | When to Use |
| --- | --- | --- |
| [Cross-layer Thinking Guide](./cross-layer-thinking-guide.md) | 检查跨层数据流、权限、事实源和 runtime projection | 功能触及 API / backend / frontend / database / local runtime 中多个层 |
| [Code Reuse Thinking Guide](./code-reuse-thinking-guide.md) | 检查是否已有可复用模式，避免重复实现 | 新增 helper、常量、组件、mapper、service 或批量相似修改 |

## Quick Triggers

- Feature touches 3+ layers -> read cross-layer guide.
- Data format changes between layers -> read cross-layer guide.
- You are modifying a constant/config -> search first.
- You are creating a helper/utility -> search first.
- You are copying a pattern -> search first, then decide whether to extract.

## Search First Rule

Before changing shared values, names, contracts, constants, or repeated patterns, search the repository:

```powershell
rg -n "value_or_name_to_change"
```

Search results help decide whether the change belongs in code only, a contract appendix, or an architecture document.

