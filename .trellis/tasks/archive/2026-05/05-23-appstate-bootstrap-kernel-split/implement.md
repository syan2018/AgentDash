# AppState Bootstrap 拆分 Implement

## Order

1. 阅读：
   - `.trellis/spec/backend/architecture.md`
   - `.trellis/spec/backend/capability/plugin-api.md`
   - `crates/agentdash-api/src/app_state.rs`
2. 新建 `crates/agentdash-api/src/bootstrap/mod.rs`，先只声明模块。
3. 提取 repository bootstrap，保持返回对象与现有字段一致。
4. 提取 plugin bootstrap，保留现有 plugin 收集和注册顺序。
5. 提取 VFS/relay/session kernel，优先移动代码，不改业务分支。
6. 提取 auth/routine/background worker bootstrap。
7. 将 `AppState::new_with_plugins` 调整为高层顺序。
8. 增加边界检查或至少补充 spec。

## Validation

```powershell
cargo check -p agentdash-api
cargo test -p agentdash-api
```

如时间允许，运行：

```powershell
cargo check --workspace
```

## Review Focus

- 移动代码时避免改变初始化顺序。
- 每个 output struct 只暴露后续步骤真实需要的字段。
- 循环/延迟注入点要集中命名，方便后续继续收敛。
