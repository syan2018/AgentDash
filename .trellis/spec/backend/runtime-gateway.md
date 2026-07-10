# Runtime Gateway

Runtime Gateway 是 actor-neutral canonical Operation 的发现、准入与派发边界。MCP、Extension、
Interaction、Workflow 与 host capability 均先投影成 exact、provider-qualified Operation，再进入统一执行核心。
该结构让调用来源、权限主体、执行位置与追踪信息保持正交，也让脚本组合能力不需要复制工具体系。

## Scenario: Canonical Operation Discovery And Invocation

### 1. Scope / Trigger

任何需要向用户或 Agent 暴露可调用能力的 provider，都通过本合同提供 descriptor、readiness 与 invoke。
HTTP、MCP、Extension bridge、Interaction command 和 Workflow node 是入口适配器，不另建调用模型。

### 2. Signatures

```text
OperationRef {
  provider_namespace,
  provider_key,
  operation_key,
  operation_version
}

OperationDescriptor {
  operation_ref,
  input_schema,
  output_schema,
  effect_summary,
  required_capabilities,
  actor_visibility,
  execution_policy,
  replay_policy,
  readiness,
  provenance
}

RuntimeInvocationEnvelope {
  operation_ref,
  input,
  principal,
  authorization_scope,
  origin,
  placement,
  trace,
  deadline,
  idempotency_key,
  optional_attachment_ref
}
```

### 3. Contracts

- Operation identity 必须包含 exact provider identity 与 version；短 key 仅可作为界面搜索文本。
- browser/iframe 只提交 `OperationRef + input`。principal、scope、placement、trace 与 capability 由 server resolver
  从可信上下文生成。
- `principal`、`authorization_scope`、`origin`、`placement`、`trace` 各自表达独立事实。
- direct invoke、OperationScript nested invoke 与 replay-safe effect invoke 共用 `OperationExecutionCore`。
- MCP 通过 `OperationMcpAccess` 获取 server-resolved AgentRun surface；connector `RuntimeSession` 只保存短期
  delivery/trace evidence。
- Extension runtime 使用 `execution_id` 关联一次执行；action、protocol 和 backend bridge 均映射到 canonical
  Operation。
- Interaction provider key 固定为 `{definition_id}.{definition_revision_id}`，确保运行实例调用 exact revision。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| OperationRef 不完整或版本不存在 | `not_found`，不做模糊匹配 |
| descriptor 对 actor 不可见 | discovery 不返回；直接调用返回 `forbidden` |
| principal/scope 无权访问 provider | `forbidden` |
| schema、capability 或 attachment 不满足 | `invalid_input` / `forbidden` |
| provider 尚未 ready | 结构化 `not_ready` |
| deadline 超时 | `deadline_exceeded` |
| replay policy 与调用模式冲突 | `conflict` |
| provider 内部失败 | 保留 operation/trace identity 的结构化错误 |

### 5. Good / Base / Bad Cases

- Good：AgentRun 只看到 attachment 与 capability 允许的 exact Operations，并以 server-resolved envelope 调用。
- Base：用户在 OperationWorkshop 中发现 project-visible Operations 并直接调用。
- Bad：iframe 自报 principal、backend 或 workspace root，或用短 key 触发任意 provider。

### 6. Tests Required

- exact identity、同名 provider 隔离与 descriptor visibility。
- human/agent principal、project scope、attachment capability admission。
- MCP、Extension、Interaction 与 Workflow adapter 到 execution core 的调用链。
- timeout、readiness、idempotency 与 replay policy。
- 浏览器 DTO 不包含 authority、placement、backend 或 session 字段。

### 7. Wrong vs Correct

```text
Boundary mismatch:
browser -> session-bound action gateway -> provider-specific execution

Canonical:
browser -> trusted context -> OperationRef -> OperationExecutionCore -> exact provider
```

## Scenario: Ephemeral OperationScript V1

### 1. Scope / Trigger

OperationScript 用于让 Agent 或用户以 Rhai 编排多次 Operation 调用，并在沙箱内筛选、归并和清理结果。
脚本文本随单次请求执行，不形成持久化领域对象。

### 2. Signatures

```text
preflight(script, trusted_context) -> {
  referenced_operations,
  diagnostics,
  estimated_limits
}

run(script, input, trusted_context) -> {
  output,
  invocation_trace,
  diagnostics
}
```

脚本仅获得受控 `op.call(operation_ref, input)`、纯数据转换与预算查询接口。

### 3. Contracts

- Rhai 是 V1 解释器；宿主以异步方式调度 nested Operation，解释器本身不获得线程、文件、网络或进程能力。
- Canvas 可将整块脚本提交 Runtime Gateway 执行；执行语义与 OperationWorkshop、Workflow 完全一致。
- nested invoke 逐次进入 `OperationExecutionCore`，继承可信 principal/scope/origin/placement/trace。
- engine limits、调用次数、输出大小、deadline 与 cancellation 由宿主强制执行。
- preflight 只解析引用与静态诊断，不执行 operation。
- 脚本和结果仅进入必要的 invocation trace；V1 不保存可复用 OperationProgram。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| Rhai 语法或类型错误 | preflight/run 返回带位置的 diagnostics |
| OperationRef 不可见或不存在 | nested invoke 返回结构化错误 |
| 超过步数、调用数、输出或时间预算 | 立即取消并返回 limit error |
| effect operation 缺少 idempotency 条件 | admission 拒绝 |
| 调用被取消 | 停止后续 nested invoke 并保留已完成 trace |

### 5. Good / Base / Bad Cases

- Good：脚本调用多个只读 Operation，过滤大结果后返回紧凑摘要。
- Base：脚本调用一个 exact Operation 并转换输出结构。
- Bad：脚本通过动态字符串枚举未授权 provider，或绕过 execution core 直连网络。

### 6. Tests Required

- preflight 引用提取、语法诊断和 zero-side-effect。
- nested async dispatch、顺序/错误传播、取消与 deadline。
- step/call/output limits，以及 host API 白名单。
- UserWorkshop、Canvas、Workflow 三类 caller 的等价 execution semantics。

### 7. Wrong vs Correct

```text
Boundary mismatch:
script -> provider SDK / filesystem / network

Canonical:
script -> op.call(exact ref) -> OperationExecutionCore -> admitted provider
```

## Extension Points

- 新 sandbox 通过 `OperationScriptEngine` 接口加入，保持 host API、预算与 execution core 合同不变。
- 新 provider 实现 descriptor/readiness/invoke 后即可被所有可信入口复用。
- 新 actor surface 只需实现 trusted context resolver 与 DTO adapter，不扩展 authority 模型。
