# 前后端协议生成标准化方案

## 结论

AgentDash 应采用 **Rust contract crate + ts-rs 生成 + check mode drift gate** 作为前后端协议构建标准。

当前 Backbone Protocol 已经证明这条路径可行：Rust 类型是事实源，TypeScript 文件生成到前端，前端直接消费生成结果。下一步不是为每个业务域各自发明 mapper，而是把 HTTP DTO、NDJSON envelope 和高漂移 enum 纳入同一条生成链路。

## 方案比较

| 方案 | 判断 |
| --- | --- |
| 在前端继续手写 DTO / enum union | 适合短期补洞，但 Workflow、Session、VFS 这类高变化域会持续产生 drift |
| 从 API route 文件直接生成类型 | 启动成本低，但 route implementation 会变成协议入口，后续难以复用到 local/desktop/测试 |
| 使用 OpenAPI 作为唯一事实源 | 适合 REST 文档，但对 Rust discriminated union、NDJSON envelope 和非 HTTP protocol 的表达不如 `ts-rs` 直接 |
| 建立 `agentdash-contracts` crate | 最符合当前架构：API、前端、流式协议和检查脚本共享一份 wire type |

推荐方案是第四种：`agentdash-contracts` 承载业务 wire DTO；`agentdash-agent-protocol` 继续承载 Backbone runtime event fact。两条协议都用同一种生成/check 机制。

## 标准链路

```text
Rust wire DTO
  -> serde JSON shape
  -> ts-rs export
  -> packages/app-web/src/generated/<domain>-contracts.ts
  -> frontend service mapper
  -> feature reducer / store / hook
```

前端 mapper 的职责保留为运行时边界校验和 view model 转换。enum/string union、字段结构和 discriminated union 由 generated type 提供。

## 第一批迁移范围

| 批次 | DTO 域 | 原因 |
| --- | --- | --- |
| 1 | MCP Preset | DTO 小、CRUD 完整、当前已有手写 TS 类型和 mapper，适合作为业务 DTO 生成样板 |
| 2 | Session stream envelope / Session context | 直接服务 NDJSON stream reducer 和 runtime surface，是 UI 稳定性的关键输入 |
| 3 | Workflow contract / activity lifecycle | 手写 normalizer 最多，后端变化频繁，收益最高但迁移面较大 |
| 4 | VFS surface / mount edit capability | Workspace panel、VFS browser、surface mutation 共用地址模型 |
| 5 | Shared Library / ProjectAgent | 资产发布、安装和 agent preset 配置跨多个 feature，适合在模式稳定后迁移 |

## 已落地的基线

- `generate_backbone_protocol_ts` 支持 `--check`。
- root `package.json` 增加：
  - `pnpm run contracts:generate`
  - `pnpm run contracts:check`
- cross-layer spec 新增 Frontend / Backend Contracts，明确生成文件、contract crate、drift check 和迁移优先级。

## 后续落地方式

1. 新增 `crates/agentdash-contracts`，先放 MCP Preset DTO。
2. `agentdash-api` 的 MCP Preset route 改用 contract DTO。
3. 生成 `packages/app-web/src/generated/mcp-preset-contracts.ts`。
4. 前端 `types/mcp-preset.ts` 改为 re-export generated type，`services/mcpPreset.ts` 只保留运行时校验和请求函数。
5. `contracts:check` 同时检查 Backbone 与 MCP Preset 生成文件。

这条路径能把“生成协议”变成可重复的工程动作，而不是一次性迁移。
