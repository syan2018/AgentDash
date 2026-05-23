---
name: trellis-update-spec
description: "Capture durable architecture invariants, current baselines, local decisions, and executable contracts into .trellis/spec/ without turning specs into task logs. Use after implementing, debugging, or discussing something that future sessions need for structural convergence."
---

# Update Spec - Maintain Architecture Attractor

Use this skill when a task reveals knowledge that future AI/developers need to keep AgentDash converging toward the right structure.

`.trellis/spec/` is not a task log. It maintains the project architecture attractor and the current engineering projection of that attractor.

---

## Spec Maintenance Goal

Spec content belongs to one of four categories:

| Category | Meaning | Maintenance Rule |
| --- | --- | --- |
| `Invariants` | Long-term structural constraints: where the system should converge | Do not auto-edit. Propose changes and ask the user to confirm. |
| `Current Baseline` | Current code projection of those invariants: crates, entry points, DTOs, registered providers, production paths | Auto-update when implementation facts change. Keep concise and factual. |
| `Local Decisions` | Stable local design choices with rationale | Auto-update only when the decision was explicitly confirmed by the task. Record why, not history. |
| `Contract Appendices` | Executable contracts: signatures, payload fields, state flows, validation/error semantics | Auto-update when contracts change. Avoid task-process material. |

Default target shape for module architecture docs:

```markdown
# <Module> Architecture

## Role
## Invariants
## Current Baseline
## Local Decisions
## Contract Appendices
```

---

## Hard Boundary: Invariants

Automatic spec maintenance must not rewrite `Invariants`.

If a learned fact appears to change a long-term invariant, stop and record an architecture-change proposal in the active task instead of editing the invariant directly. Ask the user to confirm before changing:

- module responsibility boundaries
- source-of-truth rules
- Cloud/Local ownership
- canonical state-flow or event authority
- cross-layer protocol direction
- dependency direction between layers/crates
- the meaning of a core domain abstraction

Small factual corrections in `Current Baseline` or contract appendices do not require this gate.

---

## What Belongs Where

| Learning | Target |
| --- | --- |
| "This module must converge around this source of truth" | module `architecture.md` / `Invariants` proposal |
| "This crate/function/route is the current production entry" | `Current Baseline` |
| "This DTO field / endpoint / DB column / env key changed" | contract appendix |
| "We chose Rhai/OpenAI wire API/etc. for this reason" | `Local Decisions` |
| "Run these tests for this task" | active task `implement.md` / closure, not spec |
| "This PR failed because..." | task archive / workspace journal / bug note, not spec |
| "This environment gotcha affects future agents" | `AGENTS.md` problem collection |

Do not add task-process sections such as:

- `Tests Required`
- `Good/Base/Bad Cases`
- `Wrong vs Correct`
- `Command gate`
- `Verification`
- date-based changelog
- one-off TODO / future enhancement notes

If an old task-process section contains a real contract, rewrite only the contract into concise prose or tables.

---

## Update Process

### Step 1: Identify the Durable Learning

Answer:

1. What did we learn?
2. Is it an invariant, current baseline, local decision, or contract?
3. Which module owns it?
4. Does it change an invariant?

If the answer to question 4 is yes, do not edit the invariant automatically.

### Step 2: Read the Module Architecture

Start from the module's `architecture.md`. If it does not exist, read the closest layer index and relevant appendices.

Examples:

```bash
Get-Content -Raw .trellis/spec/backend/session/architecture.md
Get-Content -Raw .trellis/spec/backend/session/session-startup-pipeline.md
```

### Step 3: Choose the Smallest Correct Edit

- Architecture docs should stay short and directional.
- Appendices can hold executable details.
- Current baseline entries should be factual and easy to check against code.
- Local decisions should explain why the chosen shape keeps the system convergent.

### Step 4: Update Indexes Only for Reading Order

Update an index when:

- a new architecture document was added
- a document moved between architecture/appendix roles
- reading order changed

Do not maintain status columns such as "created", "updated", or checkmarks.

---

## Optional Contract Appendix Template

Use this only when a concrete cross-layer or infra contract genuinely needs structure. It is not mandatory.

```markdown
## <Contract Name>

### Scope

When this contract applies.

### Signatures

Commands, routes, DTOs, DB fields, env keys, or trait methods.

### Contract

Required fields, state transitions, ownership, and serialization rules.

### Validation And Errors

Condition -> user/system error semantics.
```

Task-specific tests and closure evidence belong in the task, not here.

---

## Quality Checklist

Before finishing a spec update:

- [ ] Did I classify the update as invariant, baseline, decision, or contract?
- [ ] If it touches an invariant, did I ask for explicit confirmation instead of auto-editing?
- [ ] Is the content owned by the right module architecture or appendix?
- [ ] Did I remove task-process language?
- [ ] Did I explain why for local decisions?
- [ ] Is the resulting text useful to a future session judging structural convergence?

---

## Relationship To Finish Work

During finish-work, check whether the task changed:

- architecture invariants
- module ownership or dependency direction
- cross-layer contracts
- current production entry points
- durable local design decisions

Only then update `.trellis/spec/`. Otherwise keep the learning in task closure, journal, or AGENTS problem collection.
