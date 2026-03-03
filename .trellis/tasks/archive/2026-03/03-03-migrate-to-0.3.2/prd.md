# Migration Task: Upgrade to v0.3.2

**Created**: 2026-03-03
**From Version**: 0.2.15
**To Version**: 0.3.2
**Assignee**: unknown

## Status

- [ ] Review migration guide
- [ ] Update custom files
- [ ] Run `trellis update --migrate`
- [ ] Test workflows

---

## v0.3.0-beta.0 Migration Guide

# Migration Guide: v0.3.0 Shell to Python

## Overview

All shell scripts (.sh) have been replaced by Python scripts (.py). This affects any custom workflows, slash commands, or documentation that references the old scripts.

## Requirements

- Python 3.10 or higher

## What Changed

### Script Extensions

| Old Path | New Path |
|----------|----------|
| `.trellis/scripts/*.sh` | `.trellis/scripts/*.py` |
| `.trellis/scripts/common/*.sh` | `.trellis/scripts/common/*.py` |
| `.trellis/scripts/multi-agent/*.sh` | `.trellis/scripts/multi_agent/*.py` |

### Directory Rename

| Old | New |
|-----|-----|
| `multi-agent/` (hyphen) | `multi_agent/` (underscore) |

### Invocation Change

| Old | New |
|-----|-----|
| `./script.sh` | `python3 ./script.py` |
| `./.trellis/scripts/task.sh` | `python3 ./.trellis/scripts/task.py` |

## Files to Check

Search your project for these patterns and update them:

```bash
# Find files with old .sh references
grep -r '\.trellis/scripts.*\.sh' .claude/ .cursor/ .trellis/

# Find files with old multi-agent directory
grep -r 'multi-agent/' .claude/ .cursor/ .trellis/
```

## Common Replacements

```
# Task management
./.trellis/scripts/task.sh → python3 ./.trellis/scripts/task.py

# Context
./.trellis/scripts/get-context.sh → python3 ./.trellis/scripts/get_context.py

# Developer
./.trellis/scripts/get-developer.sh → python3 ./.trellis/scripts/get_developer.py
./.trellis/scripts/init-developer.sh → python3 ./.trellis/scripts/init_developer.py

# Session
./.trellis/scripts/add-session.sh → python3 ./.trellis/scripts/add_session.py

# Multi-agent
./.trellis/scripts/multi-agent/start.sh → python3 ./.trellis/scripts/multi_agent/start.py
./.trellis/scripts/multi-agent/status.sh → python3 ./.trellis/scripts/multi_agent/status.py
./.trellis/scripts/multi-agent/plan.sh → python3 ./.trellis/scripts/multi_agent/plan.py
./.trellis/scripts/multi-agent/cleanup.sh → python3 ./.trellis/scripts/multi_agent/cleanup.py
./.trellis/scripts/multi-agent/create-pr.sh → python3 ./.trellis/scripts/multi_agent/create_pr.py
```

## After Migration

Run `trellis update --migrate` to archive the old shell scripts to `.trellis/scripts-shell-archive/`.

### AI Assistant Instructions

When helping with this migration:

1. Search for files containing old patterns:
   - `.trellis/scripts/*.sh` references
   - `multi-agent/` directory references (should be `multi_agent/`)
   - `python script.py` without `python3` (Windows compatibility)
2. For each file found, apply the replacements from the migration guide
3. IMPORTANT: Always use `python3` explicitly, not just `python` or relying on shebang (Windows doesn't support shebang)
4. Check ALL locations: docstrings, help text, error messages, markdown docs
5. Verify the changes compile/work correctly
6. After all files are updated, remind user to run `trellis update --migrate`

---

## v0.3.0-beta.7 Migration Guide

# Migration Guide: v0.3.0-beta.7 Windows Compatibility

## Overview

This release fixes Windows compatibility issues and corrects the Claude Code hook JSON output format.

## What Changed

### Hook JSON Format (Issue #18)

SessionStart hooks now output the correct JSON format expected by Claude Code:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "..."
  }
}
```

### UTF-8 Encoding (Issue #19)

- Git commands now force UTF-8 output encoding
- Python subprocess calls use `encoding="utf-8"` and `errors="replace"`
- Cross-platform `tail_follow()` implementation in status.py

### Hook Command Change

Hook commands in `.claude/settings.json` now use `python3` directly:

```json
"command": "python3 .claude/hooks/session-start.py"
```

## Windows Users: Manual Step Required

If your Windows system uses `python` instead of `python3`, you need to manually update `.claude/settings.json`:

1. Open `.claude/settings.json`
2. Find all `"command": "python3 ..."` entries
3. Change `python3` to `python` if needed

Example:
```json
// Before (if python3 not available)
"command": "python3 .claude/hooks/session-start.py"

// After
"command": "python .claude/hooks/session-start.py"
```

## Affected Files

- `.claude/settings.json` - Hook commands updated
- `.claude/hooks/session-start.py` - JSON format fixed
- `.trellis/scripts/common/git_context.py` - UTF-8 encoding
- `.trellis/scripts/multi_agent/status.py` - Cross-platform tail
- Other scripts - UTF-8 encoding fixes

### AI Assistant Instructions

When helping with this migration:

1. Update `.claude/settings.json` hook commands to use `python3`
2. If user reports Python not found on Windows, guide them to change `python3` to `python` in settings.json
3. Verify hook scripts output correct JSON format
4. Check that git commands work with non-ASCII characters

---

## v0.3.0 Migration Guide

# Migration Guide: v0.2.x → v0.3.0

## Overview

All shell scripts (.sh) have been replaced by Python scripts (.py). This affects any custom workflows, slash commands, or documentation that references the old scripts.

## Requirements

- Python 3.10 or higher

## What Changed

### Script Extensions

| Old Path | New Path |
|----------|----------|
| `.trellis/scripts/*.sh` | `.trellis/scripts/*.py` |
| `.trellis/scripts/common/*.sh` | `.trellis/scripts/common/*.py` |
| `.trellis/scripts/multi-agent/*.sh` | `.trellis/scripts/multi_agent/*.py` |

### Directory Rename

| Old | New |
|-----|-----|
| `multi-agent/` (hyphen) | `multi_agent/` (underscore) |

### Invocation Change

| Old | New |
|-----|-----|
| `./script.sh` | `python3 ./script.py` |
| `./.trellis/scripts/task.sh` | `python3 ./.trellis/scripts/task.py` |

## After Migration

Run `trellis update --migrate` to archive the old shell scripts to `.trellis/scripts-shell-archive/`.

### AI Assistant Instructions

When helping with this migration:

When assisting users upgrading from 0.2.x to 0.3.0:
1. Search for files containing `.trellis/scripts/*.sh` references and update to `.py`
2. Replace `multi-agent/` directory references with `multi_agent/` (underscore)
3. Always use `python3` explicitly, not just `python` (Windows compatibility)
4. Check ALL locations: docstrings, help text, error messages, markdown docs
5. Run `trellis update --migrate` to remove legacy shell scripts

Note: Users upgrading from any 0.3.0 prerelease (beta/rc) need no file changes — only the version stamp is updated.

