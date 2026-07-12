# Codex App Server Protocol Codegen 工具链评估

## 结论

标准协议不应人工手抄。固定版本`codex-app-server-protocol`已经提供完整TypeScript与JSON Schema exporter、非experimental过滤、v2 flat schema bundle和fixture drift tests。AgentDash应复用该exporter，并在workspace内使用JSON Schema -> Rust生成器产出dependency-light owned标准类型。

首选Rust生成器为pinned`typify 0.7.0` builder interface。它支持从JSON Schema程序化生成持久Rust source，并允许配置rename、additional derives与replacement types。工具仅进入专用codegen crate，不进入production依赖图。

## 上游能力证据

Pinned Codex source中的`app-server-protocol/src/export.rs`提供：

- `generate_ts_with_options`
- `generate_json_with_experimental`
- `codex_app_server_protocol.schemas.json`
- `codex_app_server_protocol.v2.schemas.json`
- experimental method/field过滤

`app-server-protocol/src/schema_fixtures.rs`与对应tests提供：

- schema/TypeScript fixture树生成
- canonical JSON比较
- generated file set与内容diff
- Windows换行与schema数组稳定化

盘点时workspace仍 pinned `codex-app-server-protocol 0.140.0`（`rust-v0.140.0`）。用户已将本任务目标基线调整为官方`rust-v0.144.1 / 0.144.1`；W1必须先统一Rust/npm/revision/schema/fixture pins，再以Cargo.lock与codegen lock manifest共同固定新基线。

## Typify

- 文档：https://docs.rs/typify/latest/typify/
- 评估版本：`0.7.0`
- 使用方式：builder interface生成持久文件，而不是macro或全局`cargo typify`。
- 风险：JSON Schema可表达能力高于Rust类型系统，必须对真实Codex v2 schema先做feasibility gate；无法保真时回到设计评审，不人工维护镜像。

## 推荐命令

```powershell
cargo run -p agentdash-agent-protocol-codegen -- write
cargo run -p agentdash-agent-protocol-codegen -- check
```

## Generated/Handwritten 边界

Generated：Codex标准session/item/event/interaction payload、transitive value types、Rust与TypeScript标准子集。

Handwritten：AgentDash extension variants、Runtime durable envelope、root allowlist、extension composition、adapter method admission与业务语义测试。

禁止：人工复制Codex字段、用`serde_json::Value`代替生成失败的结构、unknown item文本化、在production build动态拉取或生成上游协议。

实际`0.144.1`生成链采用`typify 0.7.0`，并以固定schema hash约束MCP union机械override。nullable overlay使用`Variant.field`路径与schema枚举做missing/extra双向审计；owned输入允许审计字段omitted/null，输出canonical explicit null。全部generated TypeScript JSON整数必须为`number`，生成后存在`bigint`即失败。write/check共同管理Rust、TypeScript与schema roots的missing/changed/extra文件集合。
