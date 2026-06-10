# FIX-004: local-runtime ToolExecutor 边界收敛

## 模块

`local-runtime`

## 来源

- `reviews/004-local-runtime.md`
- `research/local-runtime-executable-plan.md`
- worker: `019eb2b4-9311-7e40-baf5-ea3d4bc484d6`

## 更新

- `ToolExecutor::new` 在构造期 canonicalize 并去重 workspace roots。
- 保留“roots 曾被配置”的状态，避免全部不可用时退化成任意目录可访问。
- `validate_workspace_root` 不再在运行期反复 canonicalize 登记 roots。
- `resolve_path_for_write_with_root` 只校验最近已存在父级边界，不再在解析阶段创建 parent。
- `file_write` / `file_rename` 继续在实际写入前创建目录。
- 删除搜索 fallback 链路，搜索只走 `rg`；缺少 `rg` 时返回明确 IO 错误。

## 涉及文件

- `crates/agentdash-local/src/tool_executor.rs`

## 验证

- `rg --version`：ripgrep 15.1.0。
- `cargo test -p agentdash-local tool_executor`：23 passed。
- `cargo check -p agentdash-local`：通过；仅剩既有 `agentdash-executor` unused import warning。
- `cargo fmt --check --package agentdash-local`：通过。
- `git diff --check -- crates/agentdash-local/src/tool_executor.rs`：通过。
- 汇合验证：`cargo test -p agentdash-local`：86 passed。

## Commit

待提交。
