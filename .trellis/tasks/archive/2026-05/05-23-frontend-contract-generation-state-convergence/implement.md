# 前后端契约生成与前端状态收拢 Implement

## Order

1. 盘点：
   ```powershell
   rg -n "generated|ts-rs|typeshare|normaliz|NDJSON|useSessionStream|workflowStore|storyStore" crates packages
   ```
2. 选择第一批 DTO 生成范围。
3. 增加或扩展生成脚本与 check mode。
4. 替换对应前端手写类型/normalizer。
5. 抽出一个 stream transport/reducer 切片。
6. 拆分一个 store 的纯 reducer/selector。
7. 更新 spec。

## Validation

```powershell
pnpm check
cargo check -p agentdash-agent-protocol
```

如果新增 Rust DTO export，运行对应生成脚本和 drift check。

## Review Focus

- 生成类型命名稳定。
- 前端不增加字段别名兼容层。
- reducer 是纯状态转换，可单测。
- transport 层不混入 React 生命周期。

## Progress

- 已盘点当前生成链路：Backbone Protocol 由 `agentdash-agent-protocol` 通过 `ts-rs` 生成，业务 HTTP DTO 尚未进入统一 contract crate。
- 已形成标准方案：后续新增 `agentdash-contracts` 承载业务 wire DTO，`agentdash-agent-protocol` 继续承载 runtime Backbone event fact。
- 已补 drift gate：`generate_backbone_protocol_ts --check`、`pnpm run contracts:generate`、`pnpm run contracts:check`。
- 已新增 cross-layer contract spec 与 review-round 策略文档，明确 DTO 优先级：Session stream、Workflow、VFS、MCP Preset、Shared Library / ProjectAgent。
- 已抽出 `packages/app-web/src/api/ndjsonStream.ts`，Project event stream 复用通用 NDJSON transport，业务文件只保留 project event parse/cursor。
- 已更新 frontend type-safety、state-management、hook guidelines。
- 已验证 `pnpm run frontend:check`、`pnpm run contracts:check`、`cargo check -p agentdash-agent-protocol`。
