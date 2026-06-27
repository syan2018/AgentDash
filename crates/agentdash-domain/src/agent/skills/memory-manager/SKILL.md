---
name: memory-manager
description: Manage ProjectAgent long-term memory in the default agent:// VFS home. Use when reading, creating, or updating MEMORY.md and topics/*.md so durable memory stays concise, verified, secret-free, and maintained with normal VFS file tools.
---

# Memory Manager

Use this skill when you need to read, initialize, or update AgentDash ProjectAgent memory.

The default Agent memory home is `agent://`. Treat it as shared by everyone who uses the same ProjectAgent in the same Project. Use only the normal VFS file tools for memory work: `mounts.list`, `fs.list`, `fs.read`, `fs.search`, `fs.write`, and `fs.apply_patch`.

## Layout

Keep the root index short:

```text
agent://
  MEMORY.md
  topics/
    project-decisions.md
    workflows.md
    failure-modes.md
    user-feedback.md
    external-references.md
  archive/
```

`MEMORY.md` is an index, not the knowledge body. It should contain a compact list of topic links and one-line descriptions that help future agents decide what to read.

Topic files under `topics/*.md` hold the durable content. Every topic file starts with frontmatter:

```markdown
---
name: project-decisions
description: Durable project decisions and rationale that change future implementation choices.
type: project
scope: agent
updated_at: 2026-06-24
---

# Project Decisions
```

Use `type` values from this set:

- `user`: durable user preferences or standing instructions.
- `feedback`: feedback that should change future agent behavior.
- `project`: project-specific decisions, conventions, workflows, or failure modes.
- `reference`: stable external references or pointers that future work may need.

Use `description` for future relevance selection. It should say when the topic is worth reading, not just repeat the title.

## Read Flow

1. Confirm the `agent` mount is available and inspect its capabilities with `mounts.list`.
2. Read `agent://MEMORY.md` when it exists.
3. Read only topic files that are relevant to the current task.
4. If `MEMORY.md` is missing and the mount is writable, create a short index before creating topic files.
5. If the user says to ignore memory, do not use, cite, compare, or mention memory for that request.

## Write Gate

Write memory only when the information can change a future agent's behavior.

Save:

- Stable user preferences and standing feedback.
- Project decisions with the reason and how to apply them.
- Recurring workflows, failure modes, recovery steps, or open follow-up that will matter after this session.
- External references that are durable enough to verify later.

Do not save ordinary code structure, generic architecture summaries, raw transcripts, command logs, git history, transient task details, or facts already captured in source code, docs, database state, or current task artifacts.

Convert relative dates to absolute dates before storing them.

## Update Before Create

Before writing, search existing memory:

- Read `agent://MEMORY.md`.
- Use `fs.search` or `fs.list` under `agent://topics/`.
- Update the closest existing topic with `fs.apply_patch` when possible.
- Create a new topic only when no existing topic fits.
- Keep topics semantic, not chronological.

When a new topic is needed, create the topic file first, then add or update the `MEMORY.md` index entry.

## Shared Memory Secret Scan

Before writing to `agent://`, scan the proposed text for secrets and sensitive material because ProjectAgent memory is shared.

Never store:

- API keys, tokens, passwords, private keys, cookies, session IDs, auth headers, or credential values.
- Raw personal data that is not necessary for future behavior.
- Internal URLs, hostnames, or account identifiers unless the user explicitly wants that durable reference and it is safe to share with this ProjectAgent.

If a secret influenced the work, store only the safe operational lesson, not the value.

## Stale Claim Verification

Memory is historical evidence, not current truth. Before acting on memory claims about code paths, functions, flags, configs, database fields, permissions, external resources, product behavior, or current state, verify the current facts through the relevant VFS files, runtime context, or authoritative source.

When updating a stale topic, either replace the old claim with the verified current fact or mark the old claim as superseded with the absolute date and reason.

## Maintenance

Keep memory compact:

- Prefer bullets with rationale and application notes.
- Merge duplicate topics.
- Archive low-value or outdated material under `archive/` only when it remains useful.
- Remove claims that no longer guide future behavior.
