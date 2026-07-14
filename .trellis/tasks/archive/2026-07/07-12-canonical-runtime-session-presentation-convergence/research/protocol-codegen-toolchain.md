# Codex App Server Protocol 0.144.1 Codegen 恢复审计

## 当前状态

当前分支已经引入workspace内`agentdash-agent-protocol-codegen`并将Codex Rust依赖、generator常量和integration package基线升级到`rust-v0.144.1 / 0.144.1`。这部分不能凭版本字符串宣告完成：现有connector在strict admission之后仍把完整payload压成较窄RuntimeEvent，nullable shape与root coverage也必须由W1重新验证。

## 正确边界

```text
pinned codex-app-server-protocol 0.144.1
  -> official JSON Schema / TypeScript exporter
  -> canonical schema bundle + hash/root manifest
  -> generated dependency-light owned Rust types
  -> generated frontend TypeScript
  -> Codex adapter strict transcode
  -> immutable presentation payload
```

只有codegen工具与`agentdash-integration-codex`依赖vendor crate。Managed Runtime、Application、API与frontend只依赖owned contract。

## Write/check

```powershell
cargo run -p agentdash-agent-protocol-codegen -- write
cargo run -p agentdash-agent-protocol-codegen -- check
pnpm contracts:check
```

lock manifest至少记录：

- Codex crate version/tag/commit；
- experimental flag；
- upstream schema SHA-256；
- root allowlist；
- generator/typify version；
- AgentDash extension revision；
- narrow overlay/block的schema path/hash与生成理由。

## Generated 与 manual block边界

Generated：Codex标准session/item/event/interaction/input/usage/error payload及传递依赖。

Handwritten：AgentDash extension、Runtime wrapper、root allowlist、adapter method admission、schema工具无法表达时的窄overlay/block。

窄overlay/block可以接受，但必须：

1. 绑定具体schema path/hash；
2. missing/extra fields双向审计；
3. vendor→owned→JSON与owned→JSON fixture覆盖；
4. explicit null与omitted分别有fixture；
5. 上游schema变化时check强制失败。

禁止手抄完整标准DTO、以`serde_json::Value`替代生成失败结构、unknown item文本化或production build动态生成协议。

## Main 与0.144.1关系

- main已有场景必须保持相同protected presentation body。
- `0.144.1`新增标准family可以扩展可表达空间，不得改写main已有family。
- 如果官方schema强制改变main已有JSON，W1停在G1请求明确决策；不得自行加入normalizer ignore或compatibility分支。
- AgentDash extension的JSON shape由main golden固定，不由Codex schema重命名。

## W1必须补足的证明

- fresh checkout无需全局CLI即可重建同一文件树；
- schema/root/generated file missing/changed/extra全部进入check；
- 所有纳入root的vendor↔owned JSON deep equality；
- nullable字段不被generator默认策略错误省略；
- generated TypeScript整数保持`number`且union discriminant稳定；
- Runtime/Application/frontend dependency graph不含vendor crate；
- connector mapping不在W1修改，W4负责把完整owned payload接入carrier。
