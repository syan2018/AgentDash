# Migration Task: Upgrade to v0.5.6

**Created**: 2026-05-07
**From Version**: 0.5.0-beta.3
**To Version**: 0.5.6
**Assignee**: cursor-agent

## Status

- [ ] Review migration guide
- [ ] Update custom files
- [ ] Run `trellis update --migrate`
- [ ] Test workflows

---

## v0.5.0-beta.5 Migration Guide

## Sub-Agent Rename: `implement` / `check` / `research` → `trellis-*`

beta.5 renames the three core sub-agents to add a `trellis-` prefix across all 10 agent-capable platforms (Claude, Cursor, OpenCode, Codex, Kiro, Gemini, Qoder, CodeBuddy, Copilot, Droid). The generic names (`implement`, `check`, `research`) were colliding with user-defined agents in some setups.

### What changes

| Old name | New name |
|---|---|
| `implement` | `trellis-implement` |
| `check` | `trellis-check` |
| `research` | `trellis-research` |

Paths per platform:

- **Claude / OpenCode / Cursor / etc.**: `.<platform>/agents/implement.md` → `.<platform>/agents/trellis-implement.md`
- **Codex**: `.codex/agents/implement.toml` → `.codex/agents/trellis-implement.toml`
- **Kiro**: `.kiro/agents/implement.json` → `.kiro/agents/trellis-implement.json`

### Side-effect: removed `model: opus` from agent templates

Agent YAML frontmatter no longer hardcodes a model. Falls back to whatever the platform default is. If you pinned opus explicitly for a reason (budget, performance), re-add `model: opus` to your local agent files after migrate.

### How to upgrade

```bash
trellis update --migrate
```

30 rename entries run. Unmodified agent files are renamed silently; customized ones (e.g. you tweaked the agent prompt) land on the standard `Modified by you` confirm prompt — pressing Enter (backup-rename) is safe.

### What to check after migrate

1. **Any code that spawns agents by name** — shell scripts, CI jobs, or other automation calling `Task(subagent_type="implement")` must update to `Task(subagent_type="trellis-implement")`.
2. **Aliases / hot keys** — if you had a keyboard shortcut or prompt alias that referenced `implement` / `check` / `research`, update it.
3. **Model pin** — if you customized `model: opus` in agent frontmatter, re-add it. Trellis no longer pins a model by default.

### AI Assistant Instructions

When helping with this migration:

When helping a user migrate to 0.5.0-beta.5:

1. **Grep for bare agent names in user code**: look for `Task(subagent_type="implement"|"check"|"research")` in any .md / .ts / .py / shell files. Rename each to the `trellis-` prefixed version.

2. **Check agent customizations**: diff the user's `.<platform>/agents/{implement,check,research}.*` against the new `trellis-*` templates. If they had custom content, help merge it into the new file.

3. **Model pinning**: if their agent frontmatter had `model: opus` and they still want it pinned, re-add it after migrate. beta.5 removes the default pin in favor of platform-default.

4. **Run migrate**: `trellis update --migrate`. Hash-verified renames — pristine files renamed silently, customized files land on the confirm prompt (Enter = backup-rename is safe).

5. **Verify clean second run**: after migrate, running `trellis update` again should report "Already up to date!". Any diff indicates a rename that didn't complete (user chose skip on a modified file).

