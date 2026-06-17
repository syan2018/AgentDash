# Claude Code Context Usage Reference

## Files Reviewed

- `references/claude-code/src/commands/context/context.tsx`
- `references/claude-code/src/commands/context/context-noninteractive.ts`
- `references/claude-code/src/utils/analyzeContext.ts`
- `references/claude-code/src/services/tokenEstimation.ts`
- `references/claude-code/src/context.ts`
- `references/claude-code/src/services/compact/microCompact.ts`

## Findings

- `/context` does not analyze raw REPL history. It first applies the same transforms used before the API call: compact boundary, optional context collapse, then microcompact. This makes the visualization match what the model actually receives.
- `analyzeContextUsage` builds a single context data object with categories, detail lists, model window, free space, compact buffer, and latest provider API usage.
- Categories are real accounting buckets: system prompt, system tools, MCP tools, custom agents, memory files, skills, messages, free space, and compact/autocompact buffer.
- Deferred tools are explicit. Deferred MCP/system tools can be shown as available/deferred, but excluded from actual context usage and grid occupancy.
- Tool schema token counting uses a bulk count for total tool overhead, then local proportional estimates for per-tool display. This avoids multiplying provider-side fixed tool prompt overhead.
- Skills are shown via frontmatter estimates and must not double-count the tool that exposes skills/slash commands.
- Provider API usage, when available, wins for total/current context pressure. Category estimates explain composition; they are not forced to exactly equal provider totals.
- Message detail breakdown separates tool calls, tool results, attachments, assistant text, user text, and top tools/attachments.

## Design Implications For AgentDashboard

- `SessionProjectionViewResponse.context_usage` should represent a backend-built request-view usage snapshot, not just message projection analysis.
- AgentDashboard should preserve its durable `ContextProjector` for message/compaction projection, but add a usage builder that also reads latest launch/runtime projection facts.
- `not_loaded` should not be used as a normal state. The equivalent of Claude Code deferred tools should be `deferred=true` with a meaningful source such as `tool_schema` or `mcp_server`.
- Header pressure should continue to use provider/runtime token usage; category rows should use local/backend estimates with clear source tags.
