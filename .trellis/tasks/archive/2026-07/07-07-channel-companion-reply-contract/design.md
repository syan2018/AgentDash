# Technical Design

## Core Idea

本任务采用 Companion-first 设计。核心不是提前建立通用 Channel 模型，而是把 Companion 的回复合同拆成两层：

1. `CompanionReplyContract`：runtime 内部合同，保存路由、owner、gate/pending-action 等恢复与审计所需事实。
2. `ModelReplyInstruction`：Agent 可见合同，只保存完成工具调用所需的最小信息。

未来 Channel 可以从这些已验证的词汇中生长出来；本任务不提前创建 `agentdash-domain::channel`，也不迁移 `MailboxSourceIdentity`。

## Boundary

```text
companion_request / child dispatch
  -> build CompanionReplyContract
  -> persist hidden owner refs in gate/action metadata
  -> render ModelReplyInstruction into dispatch prompt
  -> launch child with rendered prompt

child agent
  -> sees only minimal companion_respond instruction
  -> calls companion_respond(payload, optional reply_to)

companion_respond runtime
  -> resolves current CompanionReplyContract
  -> validates payload semantically
  -> executes exactly one owner operation
```

Mailbox、LifecycleGate、pending action cache、wait policy 继续拥有各自事实源。`CompanionReplyContract` 只是 companion 模块内部对这些 owner 的索引和模型可见合同生成源。

## Data Structures

Add `crates/agentdash-application/src/companion/reply_contract.rs`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompanionReplyContract {
    pub contract_id: String,
    pub target: CompanionReplyTarget,
    pub expected_payload: CompanionPayloadExpectation,
    pub model_instruction: ModelReplyInstruction,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompanionReplyTarget {
    ChildDispatch {
        correlation_ref: String,
        parent_run_id: Uuid,
        parent_agent_id: Uuid,
        child_run_id: Uuid,
        child_agent_id: Uuid,
        child_frame_id: Uuid,
        gate_id: Option<Uuid>,
    },
    ParentRequestGate {
        gate_id: Uuid,
        correlation_ref: String,
    },
    PendingAction {
        action_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompanionPayloadExpectation {
    pub expected_type: Option<String>,
    pub required_fields: Vec<String>,
    pub example_payload: serde_json::Value,
    pub repair_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelReplyInstruction {
    pub tool_name: String,
    pub minimal_arguments: serde_json::Value,
    pub reply_to: Option<ModelReplySelector>,
    pub payload_hint: CompanionPayloadExpectation,
    pub text_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelReplySelector {
    Current {
        channel: String, // e.g. "companion.parent"
    },
    Alias {
        alias: String, // e.g. "parent", "reviewer", "human"
    },
}
```

`CompanionReplyTarget` may contain GUIDs because it is not rendered to the model. `ModelReplyInstruction` is the only model-facing view. The implementation should make this hard to misuse by exposing a renderer that accepts `ModelReplyInstruction`, not the full contract.

## Dispatch Prompt Rendering

Change `build_companion_dispatch_prompt` from a free-form string builder into a renderer over a structured input:

```rust
pub struct CompanionDispatchPromptInput {
    pub plan: CompanionDispatchPlan,
    pub user_prompt: String,
    pub reply_instruction: ModelReplyInstruction,
}

pub fn build_companion_dispatch_prompt(input: CompanionDispatchPromptInput) -> String;
```

The prompt should contain:

- human-readable dispatch purpose and inherited context;
- task text;
- a `Reply Instruction` section with the minimal `companion_respond` arguments;
- payload required fields and response type guidance when known.

For the normal child dispatch path, the generated tool call example should be:

```json
{
  "payload": {
    "type": "completion",
    "status": "completed",
    "summary": "..."
  }
}
```

It should not include `dispatch_id`, `gate_id`, `run_id`, `agent_id`, `frame_id`, or `session_id`. If a future prompt genuinely provides multiple reply targets, it may include a short selector:

```json
{
  "reply_to": { "kind": "alias", "alias": "parent" },
  "payload": {
    "type": "completion",
    "status": "completed",
    "summary": "..."
  }
}
```

## `companion_respond` Tool Shape

Replace the model-facing request shape with:

```rust
pub struct CompanionRespondParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<CompanionReplySelectorParam>,
    #[schemars(schema_with = "open_response_payload_schema")]
    pub payload: serde_json::Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompanionReplySelectorParam {
    Current {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
    },
    Alias {
        alias: String,
    },
}
```

Schema rules:

- `payload` is required and must be a JSON object.
- `reply_to` is optional.
- `reply_to` accepts only `current` or `alias` in this task.
- No raw GUID selector is exposed in the model-facing schema.

Payload schema should be open:

```json
{
  "type": "object",
  "additionalProperties": true
}
```

Registered payload semantics remain in `PayloadTypeRegistry`.

## Reply Resolution

The current runtime should resolve reply target in this order:

1. If `reply_to` is omitted, load active companion reply contracts for the current runtime session/frame. Require exactly one.
2. If `reply_to.kind=current`, load the current active reply contract for the optional channel name.
3. If `reply_to.kind=alias`, resolve the alias against active reply contracts.
4. If zero or multiple contracts match, return a repairable error generated from available `ModelReplyInstruction` values.

After resolution, execute exactly one owner operation:

- `ChildDispatch` -> complete child result to parent through the child-owned gate path.
- `ParentRequestGate` -> resolve parent-owned LifecycleGate.
- `PendingAction` -> resolve pending action delivery/cache.

This replaces the current behavior that tries all three paths for the same `request_id`.

## Storage

Persist the internal `CompanionReplyContract` only where recovery needs it:

- child dispatch gate payload;
- pending action metadata if that path needs model-visible reply instructions;
- parent request gate payload if parent/human response flows rely on it.

The prompt, tool docs and skill docs must not serialize the full stored contract. They render `ModelReplyInstruction`.

Existing wait policy correlation refs remain unchanged. Existing mailbox source identity remains unchanged.

## Tool, Prompt And Skill Alignment

Update these surfaces together:

- `CompanionRespondTool::description`
- `CompanionRespondParams` JSON Schema descriptions
- Tool Schema Delta rendering only if it needs special text for optional reply selectors
- `build_companion_dispatch_prompt`
- `inject_companion_role_fragment`
- `companion-system/SKILL.md`
- `references/response-adoption.md`

All should express the same rule:

> Complete the assigned work, then call `companion_respond` with `payload`. Use `reply_to` only when the prompt lists multiple reply targets.

No model-facing instruction should mention gate/frame/run/session GUIDs.

## Channel-Lite Alignment

This task should use Channel vocabulary only where it reduces ambiguity:

- `channel` can appear as a human-readable selector namespace, such as `companion.parent`.
- `alias` is the model-facing disambiguation token.
- `correlation_ref` remains an internal owner correlation value.

Do not add generic Channel actor/message/delivery-owner structs in this task. The future Channel model should be informed by the cleaned-up Companion loop and existing mailbox source identity, not guessed ahead of time.

## Tests

Targeted tests should cover:

- `ModelReplyInstruction` renders minimal `companion_respond` args without internal GUIDs.
- `build_companion_dispatch_prompt` includes the minimal reply instruction and payload expectations.
- `companion_respond` schema accepts custom payload object with absent or unregistered `type`.
- `companion_respond` without `reply_to` resolves a singleton current child dispatch contract.
- `companion_respond` with omitted selector returns an ambiguity error when multiple contracts are active.
- error message includes the current minimal valid tool call.
- child-to-parent completion and parent request gate resolution still work through explicit resolved target.
- companion skill examples match the new tool shape and do not mention `request_id`.

## Migration Notes

No database table migration is expected. If persisting `CompanionReplyContract` into an existing JSON payload requires a schema migration for Postgres/SQLite, add it in the implementation task. Do not add `channels` or `channel_events` tables in this slice.
