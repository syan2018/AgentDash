# Rig Submodule 集成计划（SDK 版）

文档时间：2026-03-06

相关文档：
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\RUST_PI_HYBRID_DESIGN.md`
- `E:\Personal_Dev\DocsWorkspace\research\agent-design\NOTES.md`

相关仓库版本：
- `rig` @ `ac9033a6`

## 1. 文档目的

本文档专门回答两个问题：

1. 在 Rust SDK 方案里，`Rig` 应该如何被纳入工程？
2. `Rig` 仓库中，哪些模块预计会被我们**直接作为 submodule 内代码依赖**使用？

这里要先明确一个工程事实：

- **Git submodule 是仓库级概念，不是 crate 级概念**
- 因此我们实际接入方式会是：
  - 把整个 `0xPlaygrounds/rig` 仓库作为一个 git submodule 引入
  - 然后在 Cargo workspace 中按 crate 选择性引用其中的模块

所以，“哪些模块作为 submodule 引用”更准确的表述应当是：

- **整个 `rig` 仓库会作为 submodule 引入**
- **其中只有一部分 crate / 模块会被我们的 SDK 直接依赖**

## 2. 建议的 submodule 策略

## 2.1 推荐方案

建议把整个 `rig` 仓库以 git submodule 形式引入到主工程，例如：

- submodule URL：`https://github.com/0xPlaygrounds/rig.git`
- submodule path：`third_party/rig`

推荐原因：

- `rig` 本身是 workspace 仓库，内部 crate 之间已有清晰路径关系
- Rust 依赖时直接走 workspace 内部相对路径更自然
- 未来如果需要启用新的 integration crate，不需要再次调整 vendoring 方式
- 便于 pin 到稳定 commit，并进行必要的本地 patch

## 2.2 不推荐方案

### 不推荐只拷贝 `rig-core`

原因：

- 会破坏和 `rig-derive`、integration crates 的自然依赖关系
- 升级和 patch 过程更难维护
- 一旦未来需要向量库或 provider integration，就会再次拆 vendoring 方案

### 不推荐把多个 rig crate 各自当独立 submodule

原因：

- Git submodule 管理复杂度高
- `rig` 原本就是一个 workspace，不值得人为打散
- 对升级和版本一致性不利

## 3. 我们预计直接依赖的 Rig crate

在当前 SDK-first 目标下，我们预计对 `rig` 的依赖分为三层：

- **核心必需**
- **高概率启用**
- **按场景启用**

## 3.1 核心必需 crate

这些 crate 预计会被 Rust SDK 直接依赖。

### 3.1.1 `rig-core`

路径：
- `third_party/rig/rig/rig-core`

Cargo 包名：
- `rig-core`

库名：
- `rig`

用途：

- 作为整个 SDK 的 LLM 能力底座
- 提供 provider / model 抽象
- 提供 `Agent`、`PromptRequest`、`StreamingPromptRequest`
- 提供 tools、tool choice、多轮 prompt request
- 提供 structured output、RAG、streaming、hooks

在我们的方案里，`rig-core` 是**唯一确定必须直接引用**的 Rig crate。

### 3.1.2 `rig-derive`（条件必需）

路径：
- `third_party/rig/rig/rig-derive`

Cargo 包名：
- `rig-derive`

用途：

- 如果我们希望用 Rig 的 derive 宏简化 tool/schema 定义，则直接启用
- 如果我们的 SDK 最终选择全部手写 trait / schema 适配，则它可以不是首版硬依赖

判断：

- **预计高概率直接引用**
- 但它不是像 `rig-core` 那样的绝对必需项

## 3.2 高概率启用 crate

这些 crate 不是 AgentLoop 必需，但非常可能被用于支撑 SDK 的能力层。

### 3.2.1 `rig-fastembed`

路径：
- `third_party/rig/rig-integrations/rig-fastembed`

用途：

- 本地 embedding 能力
- 在不依赖外部 embedding provider 的情况下快速搭建 RAG 能力

建议：

- 如果 SDK 需要“默认可用”的本地检索能力，建议纳入
- 如果 embedding 全部由宿主业务外部提供，可暂缓

### 3.2.2 一个向量库 integration crate

在 `rig` 仓库中，可选项包括：

- `rig-lancedb`
- `rig-sqlite`
- `rig-qdrant`
- `rig-postgres`
- `rig-mongodb`
- `rig-neo4j`
- `rig-surrealdb`
- `rig-milvus`
- `rig-scylladb`
- `rig-s3vectors`
- `rig-helixdb`
- `rig-vectorize`

其中，首选建议如下：

#### 首选一：`rig-sqlite`

路径：
- `third_party/rig/rig-integrations/rig-sqlite`

适用场景：

- 本地 SDK 场景
- 单机部署
- 低门槛嵌入式存储

建议定位：

- 作为默认开发 / demo / 单机 RAG 方案

#### 首选二：`rig-lancedb`

路径：
- `third_party/rig/rig-integrations/rig-lancedb`

适用场景：

- 需要更强向量检索能力
- 本地或对象存储结合的向量检索场景

建议定位：

- 作为更偏生产/增强型的本地向量库选择

#### 首选三：`rig-qdrant`

路径：
- `third_party/rig/rig-integrations/rig-qdrant`

适用场景：

- 外部独立向量服务
- 分布式 / 服务化部署

建议定位：

- 作为远端向量检索标准接入点之一

## 3.3 按场景启用 crate

以下 crate 只在明确场景下直接引用，不作为默认依赖。

### 3.3.1 `rig-bedrock`

路径：
- `third_party/rig/rig-integrations/rig-bedrock`

适用场景：

- 明确需要 AWS Bedrock provider 生态

### 3.3.2 `rig-vertexai`

路径：
- `third_party/rig/rig-integrations/rig-vertexai`

适用场景：

- 明确需要 Vertex AI provider 生态

### 3.3.3 `rig-gemini-grpc`

路径：
- `third_party/rig/rig-integrations/rig-gemini-grpc`

适用场景：

- 明确需要 Gemini gRPC 接入

### 3.3.4 其它向量库 crates

例如：

- `rig-postgres`
- `rig-mongodb`
- `rig-neo4j`
- `rig-surrealdb`
- `rig-milvus`
- `rig-scylladb`
- `rig-s3vectors`
- `rig-helixdb`
- `rig-vectorize`

适用场景：

- 宿主系统已有既定基础设施
- 我们不希望额外建立第二套向量存储栈

## 4. 当前预计“直接引用”的 Rig 模块清单

在不引入额外宿主约束的前提下，我建议把“预计直接引用”的范围明确写成下面这组。

## 4.1 首版固定直接引用

### 仓库级 submodule

- `third_party/rig` ← 整个 `0xPlaygrounds/rig` 仓库

### crate 级固定依赖

- `third_party/rig/rig/rig-core`

这是**首版唯一必须直接依赖**的 Rig crate。

## 4.2 首版预留直接引用

如果我们在首版就需要更完整能力，优先预留这些 crate：

- `third_party/rig/rig/rig-derive`
- `third_party/rig/rig-integrations/rig-fastembed`
- `third_party/rig/rig-integrations/rig-sqlite`

这是一个非常稳妥的组合：

- `rig-core`：模型、agent、tools、streaming、hooks
- `rig-derive`：声明式工具定义
- `rig-fastembed`：本地 embedding
- `rig-sqlite`：本地向量存储

如果你们的 SDK 首版希望做到“本地即可跑通一个完整 demo + RAG 场景”，这四个 crate 基本够用。

## 4.3 第二优先级直接引用

在宿主侧明确需要时，再追加：

- `third_party/rig/rig-integrations/rig-lancedb`
- `third_party/rig/rig-integrations/rig-qdrant`
- `third_party/rig/rig-integrations/rig-bedrock`
- `third_party/rig/rig-integrations/rig-vertexai`

## 5. 哪些 Rig 模块不建议直接耦合到 SDK 核心

为了保持 SDK 核心稳定，以下 crate 不建议直接耦合进 `agent_core`。

## 5.1 各类具体向量库 integration crate

原因：

- 会把 `agent_core` 绑定到存储基础设施细节
- 应通过 `rig_bridge` 或 resource adapter 层做条件接入

建议：

- `agent_core` 只依赖抽象
- `rig_bridge` 或集成层按 feature 注入具体实现

## 5.2 provider-specific integration crate

原因：

- Bedrock / Vertex / Gemini gRPC 之类属于环境能力，不应污染 runtime 核心

建议：

- 作为 `rig_bridge` 的 feature 依赖
- 不进入 `session_sdk` 或 `agent_core` 的固定依赖集

## 6. 建议的 Cargo 依赖策略

## 6.1 Workspace 组织建议

假设主工程是一个 Rust workspace，可以这样组织：

```text
/your-workspace
  /crates
    /rig_bridge
    /agent_core
    /session_sdk
    /session_store_impls
  /third_party
    /rig   <-- git submodule
```

## 6.2 `rig_bridge` 的 Cargo 依赖建议

`rig_bridge` 直接依赖：

- `rig-core`
- 可选：`rig-derive`
- feature 可选：`rig-fastembed`
- feature 可选：`rig-sqlite`
- feature 可选：`rig-lancedb`
- feature 可选：`rig-qdrant`
- feature 可选：`rig-bedrock`
- feature 可选：`rig-vertexai`

即：

- **所有 Rig 直接依赖尽量收敛到 `rig_bridge`**
- 上层 crate 尽量不要直接依赖大量具体 Rig integration crates

## 6.3 `agent_core` 的依赖边界建议

`agent_core` 不直接依赖大部分 Rig integration crate。

建议：

- 最多仅依赖 `rig_bridge` 暴露出来的抽象接口
- 不直接感知 LanceDB / SQLite / Qdrant / Bedrock / Vertex 等细节

## 6.4 `session_sdk` 的依赖边界建议

`session_sdk` 不应直接依赖 Rig crate。

它应只依赖：

- `agent_core`
- `rig_bridge` 抽象接口
- `session_store` 抽象

## 7. 预计直接引用清单（正式声明版）

这里给出一版适合直接写进设计文档或 ADR 的正式声明。

### 7.1 仓库级引用声明

本项目预计将 `0xPlaygrounds/rig` 仓库以 git submodule 形式引入，建议挂载路径为：

- `third_party/rig`

### 7.2 首版固定 crate 级引用声明

首版 SDK 预计固定直接引用以下 Rig crate：

- `third_party/rig/rig/rig-core`

### 7.3 首版预留 crate 级引用声明

首版 SDK 预计预留以下 Rig crate 作为高概率直接引用项：

- `third_party/rig/rig/rig-derive`
- `third_party/rig/rig-integrations/rig-fastembed`
- `third_party/rig/rig-integrations/rig-sqlite`

### 7.4 二期按场景启用 crate 级引用声明

二期或按客户/宿主环境需要，再按 feature 启用以下 Rig crate：

- `third_party/rig/rig-integrations/rig-lancedb`
- `third_party/rig/rig-integrations/rig-qdrant`
- `third_party/rig/rig-integrations/rig-bedrock`
- `third_party/rig/rig-integrations/rig-vertexai`
- 其它 `rig-integrations/*` 中与宿主基础设施一致的 crate

## 8. 最终建议

如果要把这件事做稳，建议遵守下面三条：

1. **Submodule 级别只引整个 `rig` 仓库，不拆碎**
2. **Rig 的 crate 依赖尽量集中在 `rig_bridge`，不要向上层扩散**
3. **首版只把 `rig-core` 视为绝对固定依赖，其余一律按 feature 或场景追加**

这样做的好处是：

- 能最大化复用 Rig 生态
- 不会把 `agent_core` 和 `session_sdk` 绑定到具体基础设施
- 后续新增 provider / vector store / embedding 方案的成本很低

## 9. 一句话总结

**预计作为 git submodule 引入的是整个 `rig` 仓库；预计首版直接依赖的 Rig 模块，固定只有 `rig-core`，高概率追加 `rig-derive`、`rig-fastembed`、`rig-sqlite`，其余 integration crate 按场景启用。**
