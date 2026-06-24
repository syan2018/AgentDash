# Claude Code Memory Prompt Research

## Relevant Files

- `references/claude-code/src/memdir/memdir.ts`
  - `buildMemoryLines`
  - `buildMemoryPrompt`
  - `loadMemoryPrompt`
- `references/claude-code/src/memdir/memoryTypes.ts`
  - `TYPES_SECTION_COMBINED`
  - `WHAT_NOT_TO_SAVE_SECTION`
  - `WHEN_TO_ACCESS_SECTION`
  - `TRUSTING_RECALL_SECTION`
  - `MEMORY_FRONTMATTER_EXAMPLE`
- `references/claude-code/src/memdir/teamMemPrompts.ts`
  - `buildCombinedMemoryPrompt`
- `references/claude-code/src/services/extractMemories/prompts.ts`
  - `buildExtractAutoOnlyPrompt`
  - `buildExtractCombinedPrompt`
- `references/claude-code/src/services/extractMemories/extractMemories.ts`
  - `createAutoMemCanUseTool`
  - `initExtractMemories`
- `references/claude-code/src/tools/AgentTool/agentMemory.ts`
  - `AgentMemoryScope`
  - `loadAgentMemoryPrompt`
- `references/claude-code/src/skills/bundled/remember.ts`

## Prompt Patterns To Reuse

- `MEMORY.md` is a short index, not the body. Durable content lives in topic markdown files.
- Topic files carry frontmatter with at least `name`, `description`, and `type`.
- `description` is written for future relevance selection.
- Memory types are a closed taxonomy in Claude Code: `user`, `feedback`, `project`, `reference`.
- Shared/team memory has a distinct scope and must not store sensitive information.
- Memory is a historical claim. Facts about code paths, functions, flags, configs, external resources, or current state must be verified before acting on them.
- If the user says to ignore memory, the agent must not use, cite, compare, or mention memory.

## Write Rules

- Save only information that can change a future agent's behavior.
- Prefer updating an existing topic over creating a duplicate topic.
- Organize by semantic topic, not by chronological log.
- Do not save ordinary code structure, generic architecture, git history, transient task details, or facts already captured in source code, docs, database, or current task artifacts.
- Feedback/project memories should include why the rule exists and how to apply it.
- Convert relative dates to absolute dates before storing.

## Background Extraction Rules

- A background extraction agent should only inspect recent conversation evidence and the existing memory manifest.
- It should not investigate source code, grep the repo, or verify facts beyond the provided evidence.
- It should have bounded turns and memory-root-limited write access.
- If the main agent already wrote memory in the current interaction, background extraction should skip or avoid duplicate writes.

## AgentDash Implications

- `memory-manager` should be a skill that guides normal VFS file operations, not a dedicated memory API.
- Default ProjectAgent memory home should be `agent://`.
- Shared ProjectAgent memory requires secret scanning and stale fact handling because it is reused across users.
- The runtime should inject memory inventory and policy, then let the agent read relevant files through VFS.
- Claude Code is a reference for prompt and file-layout design only. This task does not support discovering local Claude Code memory and must not create a whitelist for `~/.claude` or any other local directory.
