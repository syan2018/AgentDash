# Thinking Guides

> **Purpose**: Expand your thinking to catch things you might not have considered.

---

## Why Thinking Guides?

**Most bugs and tech debt come from "didn't think of that"**, not from lack of skill:

- Didn't think about what happens at layer boundaries → cross-layer bugs
- Didn't think about code patterns repeating → duplicated code everywhere
- Didn't think about edge cases → runtime errors
- Didn't think about future maintainers → unreadable code

These guides help you **ask the right questions before coding**.

---

## Available Guides

| Guide | Purpose | When to Use |
|-------|---------|-------------|
| [Code Reuse Thinking Guide](./code-reuse-thinking-guide.md) | Identify patterns and reduce duplication | When you notice repeated patterns |
| [Cross-Layer Thinking Guide](./cross-layer-thinking-guide.md) | Think through data flow across layers | Features spanning multiple layers |

---

## ⚠️ Subagent Worktree 注意事项（含 git submodule 的项目）

本项目包含 git submodule（`third_party/vibe-kanban`、`third_party/agent-client-protocol`）。
当 Claude Code 以 `isolation: "worktree"` 派生 subagent 时，worktree 内 submodule 的 `.git`
指针文件使用相对路径，在非标准目录深度下路径解析会失败，导致 worktree 内所有 `git diff`
/ `git status` 操作报 `fatal: not a git repository`。

### 影响

- subagent 在 worktree 里无法执行任何 git 命令
- **实现本身不受影响**（文件读写正常），只有 git 操作失效

### Merge 回主分支的正确姿势

worktree 里的变更**不会自动 commit**，需要手动把文件内容同步回主工作区：

```bash
# 1. 确认 worktree 里改了哪些文件（用 find 或 agent 汇报的文件列表）
# 2. 逐文件拷贝回主工作区（Read worktree 路径 → Write 主路径）
# 3. 在主工作区正常 git add / git commit
```

也可以临时修复 worktree 内的 submodule .git 指针（改为绝对路径）再做 diff：

```bash
# 在主工作区执行，WORKTREE 替换为实际 worktree 路径
for mod in vibe-kanban agent-client-protocol; do
  echo "gitdir: $(pwd)/.git/modules/third_party/$mod" \
    > ".claude/worktrees/WORKTREE/third_party/$mod/.git"
done
```

### 长期方案

待 Claude Code 支持 worktree hook 时，在 post-create 阶段自动修复 submodule 指针。

---

## Quick Reference: Thinking Triggers

### When to Think About Cross-Layer Issues

- [ ] Feature touches 3+ layers (API, Service, Component, Database)
- [ ] Data format changes between layers
- [ ] Multiple consumers need the same data
- [ ] You're not sure where to put some logic

→ Read [Cross-Layer Thinking Guide](./cross-layer-thinking-guide.md)

### When to Think About Code Reuse

- [ ] You're writing similar code to something that exists
- [ ] You see the same pattern repeated 3+ times
- [ ] You're adding a new field to multiple places
- [ ] **You're modifying any constant or config**
- [ ] **You're creating a new utility/helper function** ← Search first!

→ Read [Code Reuse Thinking Guide](./code-reuse-thinking-guide.md)

---

## Pre-Modification Rule (CRITICAL)

> **Before changing ANY value, ALWAYS search first!**

```bash
# Search for the value you're about to change
grep -r "value_to_change" .
```

This single habit prevents most "forgot to update X" bugs.

---

## How to Use This Directory

1. **Before coding**: Skim the relevant thinking guide
2. **During coding**: If something feels repetitive or complex, check the guides
3. **After bugs**: Add new insights to the relevant guide (learn from mistakes)

---

## Contributing

Found a new "didn't think of that" moment? Add it to the relevant guide.

---

<!-- PROJECT-SPECIFIC-START: Language Requirement -->
## 语言要求

> **必须使用中文**

- 所有与用户的交流必须使用中文
- 所有文档更新必须使用中文
- 代码注释必须使用中文
- 提交信息必须使用中文

---
<!-- PROJECT-SPECIFIC-END -->

**Core Principle**: 30 minutes of thinking saves 3 hours of debugging.
