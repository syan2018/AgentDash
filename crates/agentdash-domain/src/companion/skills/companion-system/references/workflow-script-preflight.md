# Workflow Script Preflight

Use workflow script preflight when a user or companion session needs to draft a dynamic workflow before approval. The current surface validates and previews the script; runtime approval, launch, and saved workflow definitions are handled by dedicated platform commands when those commands exist.

## Contents

- Preflight Contract
- Rhai Builder Surface
- Example
- Compilation Semantics
- Diagnostic Handling

## Preflight Contract

Request:

```json
{
  "project_id": "project-uuid",
  "source_text": "workflow(#{ body: [] })",
  "args": { "topic": "orchestration" },
  "ctx": { "workspace": "demo" },
  "runtime_session_id": "optional-session-id"
}
```

Response:

- `valid`: false when any blocking diagnostic exists.
- `source_digest`: SHA-256 digest of the script source.
- `source_ref`: inline orchestration source reference.
- `raw_builder_document`: serializable document returned by Rhai evaluation.
- `plan_snapshot`: compiled `OrchestrationPlanSnapshot` when compilation succeeds.
- `plan_preview`: digest, entry nodes, node count, and node labels.
- `capability_summary`: agent procedures, API endpoints, local effect capabilities, bash commands, and human gates.
- `diagnostics`: pathful warnings and errors.

## Rhai Builder Surface

Workflow scripts are restricted Rhai builder scripts. Evaluation returns a serializable builder document and does not execute workflow side effects.

Supported helpers:

```rhai
workflow(#{ name?, args?, limits?, body, metadata? })

phase(name, [ statements ])
log(message)

agent(name, #{
  procedure?,   // existing AgentProcedure key
  prompt?,      // inline prompt compiled as a snapshot procedure
  inputs?,
  outputs?,
  limits?
})

parallel([ statements ])
pipeline([ statements ])

function(name, api_request(#{
  method,
  url,
  body?
}), #{ inputs?, outputs? })

local_effect(name, bash_exec(#{
  command,
  args?,
  working_directory?
}), #{ inputs?, outputs? })

local_effect(name, capability_effect(capability_key, input?), #{
  inputs?,
  outputs?
})

human_gate(name, #{
  form_schema,
  decision_port
})
```

`api_request.headers` is not represented by the current Function executor contract and produces a blocking diagnostic.

## Example

```rhai
workflow(#{
  name: "research_review",
  args: #{ topic: "string" },
  limits: #{ max_agents: 4, max_effects: 2, max_concurrency: 2 },
  body: [
    phase("collect", [
      parallel([
        agent("scan_docs", #{
          procedure: "researcher",
          inputs: ["topic"],
          outputs: ["notes"]
        }),
        agent("scan_code", #{
          prompt: "Inspect code for " + ctx.args.topic,
          inputs: ["topic"],
          outputs: ["code_notes"]
        })
      ]),
      pipeline([
        function("fetch_index", api_request(#{
          method: "POST",
          url: "https://example.test/index",
          body: #{ topic: ctx.args.topic }
        }), #{
          inputs: ["notes"],
          outputs: ["index"]
        }),
        local_effect("write_summary", capability_effect("workspace.write", #{
          target: "research_summary"
        }), #{
          inputs: ["index"],
          outputs: ["summary"]
        }),
        human_gate("approve_summary", #{
          form_schema: "workflow.approval",
          decision_port: "decision"
        })
      ])
    ])
  ]
})
```

## Compilation Semantics

- `phase` creates a metadata/container phase node and prefixes child node paths.
- `log` records a metadata marker and emits a warning; it does not create a runtime node.
- `pipeline` creates ordered transition rules between executable stages.
- `parallel` creates branch entry nodes; downstream stages join branch exits with `All` policy.
- `agent` with `procedure` uses an existing AgentProcedure key.
- `agent` with `prompt` and no `procedure` embeds an inline snapshot procedure.
- `function(api_request)` compiles to a Function executor API request.
- `local_effect(bash_exec)` compiles to a BashExec Function executor.
- `local_effect(capability_effect)` compiles to a LocalEffect executor.
- `human_gate` compiles to an approval gate; `decision_port` is the gate output.

Data flow is port-based. A downstream input is satisfied when an immediate predecessor exposes an output port with the same key. Entry node inputs may be supplied from script `args` or `args` schema keys.

## Diagnostic Handling

Treat error diagnostics as blocking before approval. Common diagnostics include:

- Rhai syntax or evaluation error.
- Unknown builder primitive.
- Missing required field.
- Duplicate node path.
- Empty `parallel` branches or `pipeline` stages.
- Agent without `procedure` or `prompt`.
- Unsupported API request headers.
- Missing input binding.
- Ambiguous input binding.

Warnings can still affect user expectations. For example, `log()` is metadata-only.
