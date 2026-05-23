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
