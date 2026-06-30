# 领域负载类型化标准

> 领域层 `serde_json::Value` 使用的治理规则：何时类型化、何时保留灵活性。

---

## 原则

领域层的 `serde_json::Value` 使用遵循分级治理策略：

| 优先级 | 使用场景 | 策略 |
|--------|---------|------|
| P0 | 高频业务路径（Hook payload、Tool call args、Agent config） | 必须类型化 |
| P1 | 中频路径（Workflow artifact、Rule context） | 逐步类型化 |
| P2 | 外部协议边界（Plugin API、MCP 协议适配） | 保留 `Value` 灵活性 |

---

## 类型化改造模式

### 枚举 + 通用变体

使用 `#[serde(tag = "kind")]` 内部标签，保持 JSON wire 形状稳定。`Generic { data: Value }` 变体承载未类型化 payload，并随具体变体增加逐步收窄。每次新增具体变体时从 `Generic` 中提取。

### 结构化错误

使用 `thiserror::Error` 枚举，错误变体必须携带足够上下文（如 `activity_key`、`lifecycle_id`）。

---

## 禁止模式

| 禁止 | 原因 | 正确做法 |
|------|------|---------|
| 领域层裸 `String` 错误传播 | 调用方无法做模式匹配 | 使用结构化错误枚举 |
| 高频路径直接传 `serde_json::Value` | 无编译时校验 | 定义具体结构体或枚举 |
| 为了类型化而一次性全面重写 | 风险高、回归多 | 以模块为单位逐步推进 |

---

## 保留 Value 的合法场景

以下场景允许保留 `serde_json::Value`：

- **外部扩展点**：`plugin-api` 中的插件接口，刻意保持 JSON 灵活性
- **MCP 协议适配层**：协议本身使用 JSON-RPC，桥接层不应强制类型化
- **用户可编辑配置**：Settings 等运行时可扩展的配置值
- **Tool input/output schema**：工具参数 schema 本身是 JSON Schema

---

## 错误边界规则

与 [error-handling.md](./error-handling.md) 保持一致。领域层错误必须结构化，此处额外强调：

- Workflow 校验错误使用专用枚举（如 `WorkflowValidationError`），不用裸字符串
- 错误变体必须携带足够上下文供上层做日志/展示/重试决策
- 见 [error-handling.md](./error-handling.md) 获取完整的分层错误体系
