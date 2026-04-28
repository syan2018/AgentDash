# Research: Callsite Inventory — symbols to rewrite/delete in hard cutover

- **Query**: enumerate every occurrence of `CapabilityEntry`, `CapabilityDetailedEntry`, `capabilityEntryKey`, `.capabilities` field access, `"capabilities":` JSON literal, `CAPABILITY_ALIASES` / `expand_alias` / `CAP_FILE_SYSTEM`, `include_tools` / `exclude_tools`, the `panic!("不应出现 Remove 指令")`, and `workflow_capabilities:` / `workflow_capability_directives:` naming.
- **Scope**: internal
- **Date**: 2026-04-22

Priority legend:
- **P** = production code (runtime behavior)
- **T** = test code (`#[cfg(test)]`, `pipeline_tests.rs`)
- **D** = doc / spec markdown
- **F** = fixture JSON / builtin / migration SQL

Paths are absolute Windows paths; line numbers reflect the current working tree.

---

## 1. `CapabilityEntry` (Rust type usages)

### agentdash-domain crate

| Line | File | Kind | Context |
|------|------|------|---------|
| 258 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `pub enum CapabilityEntry { Simple(String), Detailed(CapabilityDetailedEntry) }` — type definition |
| 263 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `impl CapabilityEntry {` — accessor methods (`key()`, `include_tools()`, `exclude_tools()`, `has_tool_filter()`, `simple()`, `with_excludes()`, `with_includes()`) |
| 316 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `impl fmt::Display for CapabilityEntry` |
| 322 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `impl From<&str> for CapabilityEntry` |
| 328 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `impl From<String> for CapabilityEntry` |
| 360 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `pub capabilities: Vec<CapabilityEntry>,` — field on `WorkflowContract` |
| 445 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `Add(CapabilityEntry),` — variant of `CapabilityDirective` |
| 463 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `Self::Add(CapabilityEntry::simple(key))` in `CapabilityDirective::add_simple` |
| 467 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `pub fn add_entry(entry: CapabilityEntry) -> Self` |
| 479-488 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `compute_effective_capabilities(baseline: &[CapabilityEntry], ...) -> Vec<CapabilityEntry>` signature + body (to be rewritten with slot rules) |
| 1261, 1264, 1271, 1278, 1286, 1293, 1301, 1316–1318, 1328, 1340, 1349, 1351, 1380, 1381 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **T** | Unit-test constructors — all in `mod tests` block |
| 14 | `crates/agentdash-domain/src/workflow/mod.rs` | **P** | `pub use ... CapabilityDetailedEntry, CapabilityDirective, CapabilityEntry, ContextStrategy, ...;` |

### agentdash-application crate

| Line | File | Kind | Context |
|------|------|------|---------|
| 282 | `crates/agentdash-application/src/capability/session_workflow_context.rs` | **T** | `use ... CapabilityDirective, CapabilityEntry, LifecycleDefinition, ...` — inside test module |
| 577 | `crates/agentdash-application/src/capability/session_workflow_context.rs` | **T** | `capabilities: vec![CapabilityEntry::simple("workflow_management")]` — test fixture constructing `WorkflowContract` |
| 963 | `crates/agentdash-application/src/capability/session_workflow_context.rs` | **T** | `CapabilityEntry::simple("workflow_management")` |
| 964 | `crates/agentdash-application/src/capability/session_workflow_context.rs` | **T** | `CapabilityEntry::simple("file_system")` |
| 965 | `crates/agentdash-application/src/capability/session_workflow_context.rs` | **T** | `CapabilityEntry::simple("mcp:code_analyzer")` |
| 984 | `crates/agentdash-application/src/capability/session_workflow_context.rs` | **P** | `CapabilityDirective::Add(entry) => entry.key().to_string(),` |
| 985 | `crates/agentdash-application/src/capability/session_workflow_context.rs` | **P** | `CapabilityDirective::Remove(key) => panic!("不应出现 Remove 指令: {key}"),` — the panic callsite (see §7) |
| 17 | `crates/agentdash-application/src/capability/pipeline_tests.rs` | **T** | `use agentdash_domain::workflow::{CapabilityDirective, CapabilityEntry, compute_effective_capabilities};` |
| 47, 48 | `crates/agentdash-application/src/capability/pipeline_tests.rs` | **T** | `CapabilityEntry::simple("file_system")`, `CapabilityEntry::simple("collaboration")` |
| 105–107 | `crates/agentdash-application/src/capability/pipeline_tests.rs` | **T** | `CapabilityEntry::simple("file_system" / "canvas" / "collaboration")` |
| 175–176 | `crates/agentdash-application/src/capability/pipeline_tests.rs` | **T** | `CapabilityEntry::simple("file_system" / "workflow")` |
| 192 | `crates/agentdash-application/src/capability/pipeline_tests.rs` | **T** | `let baseline = vec![CapabilityEntry::simple("file_system")];` |
| 66 | `crates/agentdash-application/src/workflow/step_activation.rs` | **P** | `pub baseline_override: Option<Vec<agentdash_domain::workflow::CapabilityEntry>>,` |
| 132 | `crates/agentdash-application/src/workflow/step_activation.rs` | **P** | `let baseline: Vec<agentdash_domain::workflow::CapabilityEntry> = ...` |
| 136 | `crates/agentdash-application/src/workflow/step_activation.rs` | **P** | `.map(|w| w.contract.capabilities.clone())` — field access on `WorkflowContract` |
| 153 | `crates/agentdash-application/src/workflow/step_activation.rs` | **P** | `.map(CapabilityDirective::add_entry)` |
| 389, 445, 446, 476, 477, 493 | `crates/agentdash-application/src/workflow/step_activation.rs` | **T** | Test helpers and assertions using `CapabilityEntry::simple(...)` |
| 217 | `crates/agentdash-application/src/vfs/tools/provider.rs` | **P** | `// 工具级排除：从 CapabilityEntry 的 include/exclude 合并而来` — comment only (no symbol reference) |

### agentdash-mcp crate

| Line | File | Kind | Context |
|------|------|------|---------|
| 418 | `crates/agentdash-mcp/src/servers/workflow.rs` | **P** | `.map(|s| agentdash_domain::workflow::CapabilityEntry::simple(s))` — `build_contract` adapter in `upsert_workflow_tool` |

### Frontend TypeScript

| Line | File | Kind | Context |
|------|------|------|---------|
| 142–150 | `frontend/src/types/workflow.ts` | **P** | `export type CapabilityEntry = string \| { key: string; include_tools?; exclude_tools? }` — type definition |
| 153 | `frontend/src/types/workflow.ts` | **P** | `export function capabilityEntryKey(entry: CapabilityEntry): string` |
| 210 | `frontend/src/types/workflow.ts` | **P** | `capabilities: CapabilityEntry[]` — field on `WorkflowContract` TS interface |
| 4 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `CapabilityEntry,` — import from `../../types/workflow` |
| 482, 538, 539, 544, 549, 606, 607, 705 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | Type annotations on helpers (`getExcludedTools`, `findEntryByKey`, `upsertEntry`) and props types |

---

## 2. `CapabilityDetailedEntry`

| Line | File | Kind | Context |
|------|------|------|---------|
| 242 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `pub struct CapabilityDetailedEntry { key, include_tools, exclude_tools }` — struct definition |
| 260 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `Detailed(CapabilityDetailedEntry)` variant of `CapabilityEntry` |
| 299 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `Self::Detailed(CapabilityDetailedEntry { ... exclude_tools: excludes })` in `with_excludes` |
| 308 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `Self::Detailed(CapabilityDetailedEntry { ... include_tools: includes })` in `with_includes` |
| 14 | `crates/agentdash-domain/src/workflow/mod.rs` | **P** | re-export in `pub use` |

No frontend usages (TS `CapabilityEntry` object variant encodes it inline).

---

## 3. `capabilityEntryKey` (frontend helper)

| Line | File | Kind | Context |
|------|------|------|---------|
| 153 | `frontend/src/types/workflow.ts` | **P** | Function definition |
| 21 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `import { capabilityEntryKey } from "../../types/workflow";` |
| 545 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `return capabilities.find((e) => capabilityEntryKey(e) === key);` |
| 550 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `const idx = capabilities.findIndex((e) => capabilityEntryKey(e) === key);` |
| 645 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `const key = capabilityEntryKey(entry);` |
| 659 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `onChange(capabilities.filter((e) => capabilityEntryKey(e) !== key));` |
| 669 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `onChange(capabilities.filter((e) => capabilityEntryKey(e) !== compositeKey));` |
| 676 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `onChange(capabilities.filter((e) => capabilityEntryKey(e) !== key));` |

---

## 4. `.capabilities` field access on `WorkflowContract` / draft (Rust + TS)

> Note: this section filters out unrelated `.capabilities` usages (VFS mount capabilities, backend capabilities, container capabilities, story context capabilities). Only `WorkflowContract.capabilities` / `draft.contract.capabilities` are listed.

### Rust — production paths

| Line | File | Kind | Context |
|------|------|------|---------|
| 136 | `crates/agentdash-application/src/workflow/step_activation.rs` | **P** | `.map(\|w\| w.contract.capabilities.clone())` — default baseline resolution |
| 195 | `crates/agentdash-application/src/workflow/definition.rs` | **P** | `.capabilities` field read inside builtin loader |
| 264 | `crates/agentdash-application/src/capability/session_workflow_context.rs` | **P** | `.capabilities` read when building session baseline from active workflow |
| 414 | `crates/agentdash-mcp/src/servers/workflow.rs` | **P** | `input.capabilities` — `WorkflowContractInput` field (MCP upsert path, maps to domain) |
| 424 | `crates/agentdash-application/src/workflow/orchestrator.rs` | **P** | `.capabilities` read inside `resolve_step_workflow_capability_directives` |
| 499 | `crates/agentdash-application/src/session/plan.rs` | **P** | `.capabilities` read from workflow for plan step |
| 743 | `crates/agentdash-api/src/routes/workflows.rs` | **P** | `.capabilities` read in HTTP route serialization |

### Rust — serde serialization via struct layout

- `crates/agentdash-domain/src/workflow/value_objects.rs:360` — `pub capabilities: Vec<CapabilityEntry>` on `WorkflowContract` controls both `WorkflowContract` JSON `"capabilities"` key and the Rust field-access surface above.

### Rust — tests / builtin-JSON round-trip fixtures

| Line | File | Kind | Context |
|------|------|------|---------|
| 1370, 1387, 1395, 1396, 1403, 1404, 1405, 1406 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **T** | Assertions `contract.capabilities.len()` / `contract.capabilities[i].*()` |
| 188 | `crates/agentdash-application/src/workflow/definition.rs` | **P** | Comment reference (`// capability 声明迁移到 workflow.contract.capabilities`) |
| 984 | `crates/agentdash-application/src/session/assembler.rs` | **P** | Comment: `// 3. 查 workflow 定义 → contract.capabilities` |

### Frontend TS

| Line | File | Kind | Context |
|------|------|------|---------|
| 210 | `frontend/src/types/workflow.ts` | **P** | Field declaration on `WorkflowContract` TS interface |
| 211 | `frontend/src/services/workflow.ts` | **P** | `capabilities: asStringArray(value.capabilities),` — DTO mapper in `mapWorkflowContract` |
| 106 | `frontend/src/stores/workflowStore.ts` | **P** | `capabilities: []` — initial empty value in `newContract()` default |
| 1035 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `title={\`Agent 工具能力 (${draft.contract.capabilities.length})\`}` |
| 1040 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `capabilities={draft.contract.capabilities}` — prop passed to `CapabilitiesEditor` |

---

## 5. `"capabilities":` literal in JSON / SQL / spec fixtures

### Builtin JSON fixtures

| Line | File | Kind | Context |
|------|------|------|---------|
| 21 | `crates/agentdash-application/src/workflow/builtins/builtin_workflow_admin.json` | **F** | `"capabilities": ["workflow_management"]` in plan workflow's contract |
| 38 | `crates/agentdash-application/src/workflow/builtins/builtin_workflow_admin.json` | **F** | `"capabilities": ["workflow_management"]` in apply workflow's contract |

`crates/agentdash-application/src/workflow/builtins/trellis_dag_task.json` — **no `capabilities` key**; skip.

### In-code JSON literals (Rust)

| Line | File | Kind | Context |
|------|------|------|---------|
| 1393 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **T** | `r#"{"capabilities":["file_system","workflow_management"]}"#` — deserialization test |
| 1401 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **T** | `r#"{"capabilities":["file_system",{"key":"file_read","exclude_tools":["fs_grep"]}]}"#` — deserialization test |
| 465 | `crates/agentdash-application/src/workflow/tools/advance_node.rs` | **P** | `"capabilities": new_caps_set.iter().cloned().collect::<Vec<_>>()` — runtime emits snapshot with this key name |
| 358 | `crates/agentdash-application/src/session/prompt_pipeline.rs` | **P** | `"capabilities": initial_caps.iter().collect::<Vec<_>>()` — prompt-pipeline telemetry key |

### SQL migration

| Line | File | Kind | Context |
|------|------|------|---------|
| 40 | `crates/agentdash-infrastructure/migrations/0016_workflow_contract_capabilities.sql` | **F** | `step_caps := step_item -> 'capabilities';` |
| 52 | `crates/agentdash-infrastructure/migrations/0016_workflow_contract_capabilities.sql` | **F** | `existing_caps := COALESCE(wf_contract -> 'capabilities', '[]'::jsonb);` |
| 65 | `crates/agentdash-infrastructure/migrations/0016_workflow_contract_capabilities.sql` | **F** | `wf_contract := jsonb_set(wf_contract, '{capabilities}', merged_caps, true);` |

### Spec docs (markdown)

| Line | File | Kind | Context |
|------|------|------|---------|
| 106, 161, 164, 182, 193, 208, 222, 238, 265 | `.trellis/spec/backend/capability/tool-capability-pipeline.md` | **D** | Multiple references to `workflow_capability_directives` / `capability_directives_from_active_workflow` |
| 207, 279, 280, 281 | `.trellis/spec/backend/capability/tool-capability-pipeline.md` | **D** | Mentions of `workflow.contract.capabilities` |

(Other `.trellis/spec/**` scanned: only vfs-access.md has an unrelated `capabilities: CapSet` — ignore.)

---

## 6. `CAPABILITY_ALIASES` / `expand_alias` / `CAP_FILE_SYSTEM`

### Rust — constants & alias logic

| Line | File | Kind | Context |
|------|------|------|---------|
| 75 | `crates/agentdash-spi/src/tool_capability.rs` | **P** | `pub const CAP_FILE_SYSTEM: &str = "file_system";` |
| 107 | `crates/agentdash-spi/src/tool_capability.rs` | **P** | `pub const CAPABILITY_ALIASES: &[(&str, &[&str])] = &[ ... ];` |
| 108 | `crates/agentdash-spi/src/tool_capability.rs` | **P** | `(CAP_FILE_SYSTEM, &[CAP_FILE_READ, CAP_FILE_WRITE, CAP_SHELL_EXECUTE]),` |
| 112 | `crates/agentdash-spi/src/tool_capability.rs` | **P** | `pub fn expand_alias(key: &str) -> Option<&'static [&'static str]>` |
| 113 | `crates/agentdash-spi/src/tool_capability.rs` | **P** | `CAPABILITY_ALIASES.iter()` inside `expand_alias` |
| 121 | `crates/agentdash-spi/src/tool_capability.rs` | **P** | `is_known_key` uses `CAPABILITY_ALIASES` |
| 256 | `crates/agentdash-spi/src/tool_capability.rs` | **P** | `if let Some(expanded) = expand_alias(key) { ... }` — inside `tools_for_capability` |
| 283 | `crates/agentdash-spi/src/tool_capability.rs` | **P** | `CAP_FILE_SYSTEM => vec![ToolCluster::Read, ToolCluster::Write, ToolCluster::Execute]` — `clusters_for_capability` fallback |
| 436 | `crates/agentdash-spi/src/tool_capability.rs` | **P** | `if expand_alias(cap.key()).is_some() { ... }` — `validate_capability_key` |
| 461 | `crates/agentdash-spi/src/tool_capability.rs` | **T** | `ToolCapability::new(CAP_FILE_SYSTEM)` test |
| 496 | `crates/agentdash-spi/src/tool_capability.rs` | **T** | same |
| 502–505 | `crates/agentdash-spi/src/tool_capability.rs` | **T** | `fn expand_alias_file_system() { let expanded = expand_alias("file_system").unwrap(); ... }` |
| 558 | `crates/agentdash-spi/src/tool_capability.rs` | **T** | `ToolCapability::new(CAP_FILE_SYSTEM)` test |
| 7 | `crates/agentdash-application/src/capability/tool_catalog.rs` | **P** | `use agentdash_spi::tool_capability::{ ... expand_alias ... };` |
| 21 | `crates/agentdash-application/src/capability/tool_catalog.rs` | **P** | `if let Some(expanded) = expand_alias(key) { ... }` |
| 13 | `crates/agentdash-application/src/capability/resolver.rs` | **P** | `use agentdash_spi::tool_capability::{ ... expand_alias, ... };` |
| 132 | `crates/agentdash-application/src/capability/resolver.rs` | **P** | `if let Some(expanded_keys) = expand_alias(key) { ... }` inside `Add` arm |
| 197 | `crates/agentdash-application/src/capability/resolver.rs` | **P** | `if let Some(expanded_keys) = expand_alias(key) { ... }` inside `Remove` arm |
| 14 | `crates/agentdash-application/src/capability/notification.rs` | **P** | `use agentdash_spi::tool_capability::{ ... CAP_FILE_SYSTEM, ... };` |
| 21 | `crates/agentdash-application/src/capability/notification.rs` | **P** | `CAP_FILE_SYSTEM => "文件读 / 写 / 执行"` in human-readable label map |

### Frontend TS

| Line | File | Kind | Context |
|------|------|------|---------|
| 176–178 | `frontend/src/types/workflow.ts` | **P** | `export const CAPABILITY_ALIASES: Record<string, string[]> = { file_system: [...] };` |

---

## 7. `include_tools` / `exclude_tools` field accesses (case-sensitive)

### Rust production

| Line | File | Kind | Context |
|------|------|------|---------|
| 246, 249 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `pub include_tools: Vec<String>,` / `pub exclude_tools: Vec<String>,` on `CapabilityDetailedEntry` |
| 271–288 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | Accessor impls `include_tools()`, `exclude_tools()`, `has_tool_filter()` |
| 301, 302, 310, 311 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **P** | `with_excludes` / `with_includes` builders |
| 141, 142, 155, 156 | `crates/agentdash-application/src/capability/resolver.rs` | **P** | `.include_tools()` / `.exclude_tools()` calls when building `ToolFilter` |
| 257, 258 | `crates/agentdash-application/src/capability/resolver.rs` | **P** | Local struct `ToolFilter { include_tools: Vec<String>, exclude_tools: Vec<String> }` |
| 263, 264, 272, 275, 282 | `crates/agentdash-application/src/capability/resolver.rs` | **P** | Filter-application logic reading both vecs |

### Rust tests

| Line | File | Kind | Context |
|------|------|------|---------|
| 1281, 1356, 1406 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **T** | Assertions on `exclude_tools()` |
| 1401 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **T** | JSON literal `{"key":"file_read","exclude_tools":["fs_grep"]}` |
| 255, 353 | `crates/agentdash-domain/src/workflow/value_objects.rs` | **D** | Doc comment examples |

### Frontend TS

| Line | File | Kind | Context |
|------|------|------|---------|
| 147, 149 | `frontend/src/types/workflow.ts` | **P** | `include_tools?: string[]; exclude_tools?: string[]` on object variant |
| 206 | `frontend/src/types/workflow.ts` | **D** | Comment example |
| 540 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `return typeof entry === "string" ? [] : entry.exclude_tools ?? [];` |
| 707 | `frontend/src/features/workflow/workflow-editor.tsx` | **P** | `: { key: capKey, exclude_tools: newExcluded };` — object literal build |

---

## 8. `panic!("不应出现 Remove 指令")`

- **Single callsite**: `crates/agentdash-application/src/capability/session_workflow_context.rs:985`
- Surrounding context (lines 983-985):
  ```
  match d {
      CapabilityDirective::Add(entry) => entry.key().to_string(),
      CapabilityDirective::Remove(key) => panic!("不应出现 Remove 指令: {key}"),
  }
  ```
- Match arm is inside a closure that maps `workflow_capability_directives` → key strings for further baseline assembly. The implementer must delete this arm (or handle `Remove` legitimately) as part of the new slot-based reducer.

---

## 9. `workflow_capabilities:` / `workflow_capability_directives:` naming (for context only)

These field names on `SessionWorkflowContext` and callsites are **not** scheduled for deletion (they're the directive transport, and `capability_directives` is the *adopted* term). Kept here so the implementer can verify consistency when renaming `WorkflowContract.capabilities` → `WorkflowContract.capability_directives`.

### Current production sites using `workflow_capability_directives`

| File | Line(s) |
|------|---------|
| `crates/agentdash-application/src/capability/session_workflow_context.rs` | 3, 30, 37, 203, 235, 631, 883, 954 |
| `crates/agentdash-application/src/capability/resolver.rs` | 42, 44, 46, 123, 566, 589, 607, 644, 677, 688, 701, 724, 736 |
| `crates/agentdash-application/src/session/assembler.rs` | 451, 455, 649, 940 |
| `crates/agentdash-application/src/task/session_runtime_inputs.rs` | 71, 87 |
| `crates/agentdash-application/src/workflow/orchestrator.rs` | 205, 409 |
| `crates/agentdash-application/src/workflow/tools/advance_node.rs` | 377 |
| `crates/agentdash-application/src/workflow/step_activation.rs` | 149, 494 |
| `crates/agentdash-application/src/capability/pipeline_tests.rs` | 8, 72, 139, 209 |
| `.trellis/spec/backend/capability/tool-capability-pipeline.md` | multiple (spec doc) |

### Legacy `workflow_capabilities:` name (no longer on current `SessionWorkflowContext` struct — only appears in historical task PRD text under `.trellis/tasks/04-20-session-workflow-context-wiring/prd.md`). Not a rename target.

---

## Caveats / Not Found

- `CapabilityDetailedEntry` is referenced only in `value_objects.rs` + `mod.rs` re-export; no downstream consumers construct it directly (everyone goes through `CapabilityEntry::with_excludes/with_includes`).
- `CAP_FILE_SYSTEM` is referenced in `notification.rs` for a UI label (`"文件读 / 写 / 执行"`). After the alias goes away, this human-label mapping will need three separate entries for `file_read/file_write/shell_execute` — flagging because it's easy to miss.
- Frontend has a top-level alias constant but **no `expand_alias` function**; aliasing only affects runtime string equality checks.
- Session assembler and task_session_runtime_inputs currently consume **directives** (not entries) — renaming `capabilities` → `capability_directives` on the domain struct will propagate cleanly there, but their existing `resolve_owner_workflow_capability_directives` fn name is already aligned.
- The MCP `upsert_workflow_tool` `WorkflowContractInput.capabilities: Option<Vec<String>>` (crates/agentdash-mcp/src/servers/workflow.rs:73) accepts **plain strings only today** — migration of this MCP input shape to accept `Vec<CapabilityDirective>` is part of the cutover (see PRD §① / §③ section on upsert).
- No fixture under `.trellis/spec/frontend/` mentions CapabilityEntry — no frontend spec doc rewrite needed.
