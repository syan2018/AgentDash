# Workflow / Lifecycle 数据结构提案

## 1. 设计结论

推荐拆成两个独立实体：

* `WorkflowDefinition`
  * 表示一个原子行为 contract
* `LifecycleDefinition`
  * 表示 agent 生命周期编排器

不再推荐让 `lifecycle` 继续作为 `WorkflowDefinition.kind` 的一种。

原因很简单：

* 一个 lifecycle step 本质上像是在“指派一个 workflow”
* 一个普通 workflow 也可能被动态 attach 到 session
* 如果把 lifecycle 塞回 workflow 顶层，边界会再次变糊

---

## 2. WorkflowDefinition

```ts
type WorkflowDefinition = {
  key: string
  name: string
  description: string

  targetScope: WorkflowTargetScope
  recommendedRole?: WorkflowAgentRole

  contract: WorkflowContract

  attachmentDefaults?: WorkflowAttachmentDefaults
  metadata?: Record<string, unknown>
}
```

### 解释

单个 workflow 只解决一件事：

* “当这个 workflow 激活时，agent 应该如何行为”

它不负责多阶段流转。

---

## 3. WorkflowContract

```ts
type WorkflowContract = {
  goal?: GoalSpec
  context?: ContextSpec[]
  constraints?: ConstraintSpec[]
  checks?: CheckSpec[]
  outputs?: OutputSpec[]
  stopPolicy?: StopPolicySpec
  notes?: string[]
}
```

### 解释

这是 workflow 的核心。

无论是：

* lifecycle step 里的主 workflow
* 动态 attach 的行为 workflow
* 子 agent 回流后形成的 overlay workflow

最终都应该投影成同一种 contract。

---

## 4. LifecycleDefinition

```ts
type LifecycleDefinition = {
  key: string
  name: string
  description: string

  targetScope: WorkflowTargetScope
  recommendedRole?: WorkflowAgentRole

  activation: LifecycleActivationPolicy
  entryStep: string
  steps: LifecycleStepDefinition[]

  metadata?: Record<string, unknown>
}
```

### 解释

lifecycle 只解决：

* 什么时候进入哪个 step
* 当前 step 应指派哪个 workflow
* 是否附加其他 overlay workflows
* 何时切换 step
* step 失败后怎么办

---

## 5. LifecycleStepDefinition

```ts
type LifecycleStepDefinition = {
  key: string
  title: string
  description?: string

  sessionPolicy?: SessionPolicySpec

  primaryWorkflowRef: WorkflowRef
  attachedWorkflowRefs?: LifecycleAttachmentSpec[]

  transition: LifecycleTransitionSpec
}
```

### 解释

每个 lifecycle step 至少要回答：

* 主 workflow 是谁
* 还要挂哪些辅助 workflows
* 什么时候离开这个 step

也就是说，旧模型里的 phase 可以理解为：

* lifecycle 的一个 step
* step 内部引用了一个 primary workflow

---

## 6. 动态 attach workflow

用户提到的这类需求：

* 不分 phase
* 用于定义标准 agent 行为
* 在 agent 运行中动态注入

推荐不要做成特殊类型，而是普通 workflow + attachment：

```ts
type WorkflowAttachment = {
  workflowRef: WorkflowRef
  mode: "primary" | "overlay"
  lifetime: "turn" | "session" | "until_resolved" | "until_step_exit"
  priority?: number
  mergePolicy?: MergePolicySpec
  reason?: string
}
```

### 典型场景

* 标准 code review 输出 workflow
* 标准 research 输出 workflow
* 标准 record / handoff workflow
* blocking review adoption workflow

---

## 7. ContextSpec

```ts
type ContextSpec =
  | { kind: "document"; locator: string; required?: boolean; title?: string }
  | { kind: "runtime_context"; locator: string; required?: boolean; title?: string }
  | { kind: "checklist_source"; locator: string; required?: boolean; title?: string }
  | { kind: "artifact_ref"; locator: string; required?: boolean; title?: string }
  | { kind: "action_ref"; locator: string; required?: boolean; title?: string }
```

### 解释

只表达输入来源，不表达行为治理。

---

## 8. ConstraintSpec

```ts
type ConstraintSpec =
  | { kind: "deny_tool"; tools: string[] }
  | { kind: "rewrite_tool_arg"; tool: string; arg: string; strategy: string }
  | { kind: "require_approval"; tools: string[] }
  | { kind: "deny_task_status_transition"; to: string[] }
  | { kind: "require_artifact_before_exit"; artifactTypes: string[] }
  | { kind: "block_stop_until_checks_pass" }
  | { kind: "subagent_adoption_mode"; mode: "suggestion" | "follow_up_required" | "blocking_review" }
  | { kind: "require_output_section"; sections: string[] }
  | { kind: "require_output_artifact"; artifactTypes: string[] }
  | { kind: "enforce_response_style"; style: string }
  | { kind: "custom_constraint"; key: string; payload?: Record<string, unknown> }
```

### 解释

表达“agent 被禁止/要求做什么”。

---

## 9. CheckSpec

```ts
type CheckSpec =
  | { kind: "task_status_in"; statuses: string[] }
  | { kind: "artifact_exists"; artifactType: string }
  | { kind: "artifact_count_gte"; artifactType: string; minCount: number }
  | { kind: "session_terminal_in"; states: string[] }
  | { kind: "pending_action_none"; actionTypes?: string[] }
  | { kind: "subagent_result_status_in"; statuses: string[] }
  | { kind: "custom_check"; key: string; payload?: Record<string, unknown> }
```

### 解释

表达“当前 workflow 如何判断达成”。

---

## 10. OutputSpec

```ts
type OutputSpec =
  | {
      kind: "message_shape"
      sections: Array<{ key: string; title?: string; required: boolean }>
    }
  | {
      kind: "artifact_shape"
      artifactType: string
      title?: string
      required: boolean
    }
  | {
      kind: "structured_result"
      schemaKey: string
      required: boolean
    }
```

### 解释

这是“标准化产出”进入正式模型的关键。

如果没有这层，系统又会退回：

* prompt 里写“请按这个格式回答”
* runtime 里靠猜测来判定 agent 是否遵守

---

## 11. LifecycleTransitionSpec

```ts
type LifecycleTransitionSpec = {
  policy:
    | { kind: "manual"; nextStep?: string }
    | { kind: "all_checks_pass"; nextStep?: string }
    | { kind: "any_checks_pass"; nextStep?: string }
    | { kind: "session_terminal_matches"; states: string[]; nextStep?: string }
    | { kind: "explicit_action"; actionKey: string; nextStep?: string }

  onFailure?: {
    action: "stay" | "block" | "retry" | "fail_lifecycle"
  }
}
```

### 解释

旧的 `manual / session_ended / checklist_passed` 最终应退化为 transition preset，而不是 workflow 的核心字段。

---

## 12. EffectiveSessionContract

session 在运行时真正消费的，不应是原始 workflow/lifecycle，而应是合成后的结果：

```ts
type EffectiveSessionContract = {
  lifecycle?: {
    lifecycleKey: string
    activeStepKey: string
  }

  attachments: WorkflowAttachment[]

  goal?: GoalSpec
  context: ContextSpec[]
  constraints: ConstraintSpec[]
  checks: CheckSpec[]
  outputs: OutputSpec[]
}
```

### 合成来源

1. 当前 active lifecycle step 的 primary workflow
2. 当前 step 默认挂载的 overlay workflows
3. session 运行中动态 attach 的 workflows
4. pending action / subagent return 形成的 runtime overlays

---

## 13. 合成规则

推荐默认：

* `context`：append
* `constraints`：union
* `checks`：append
* `outputs`：append

高优先级 overlay 可以通过 `mergePolicy` 显式 replace 某些输出要求。

---

## 14. 与当前项目的迁移建议

### 第一步

扩展 `WorkflowDefinition`，让它先能表达：

* `constraints`
* `checks`
* `outputs`

### 第二步

新增 `LifecycleDefinition`，把旧 phase 顺序迁进去：

* `start`
* `implement`
* `check`
* `record`

每个 step 都引用一个 workflow。

### 第三步

挑两个试点：

* 一个 lifecycle step workflow：`task_check_workflow`
* 一个动态 attach workflow：`standard_review_output_workflow`

---

## 15. 一句话总结

推荐的数据结构不再是“一个 workflow 里套 phases”，而是：

* `WorkflowDefinition` 负责定义原子行为 contract
* `LifecycleDefinition` 负责按步骤编排这些 workflows
* runtime 永远只消费 `EffectiveSessionContract`
