# fs_read 图片 Block 返回能力执行计划

## Checklist

- [x] 在 SPI 增加 `BinaryReadResult` 与 `MountProvider::read_binary` 默认契约。
- [x] 在 RelayVfsService 增加 `read_binary` 路由，复用 mount/path/identity 解析。
- [x] 为 `inline_fs` 实现 `read_binary`，补 provider test。
- [x] 为 `skill_asset_fs` 实现 `read_binary`，补 provider test。
- [x] 更新 `fs_read`：stat binary metadata、image block、unsupported binary、文本行为不回退。
- [x] 更新 Codex app protocol stream mapper，Image 转 data URL。
- [x] 更新 Anthropic bridge 的 tool_result content 映射，保留图片 block。
- [x] 更新 VFS spec 记录 Agent `fs.read` 的图片只读契约。
- [x] 运行格式化、类型检查、相关单元测试与 diff 检查。

## Validation

```text
cargo fmt
cargo check -p agentdash-api
cargo test -p agentdash-application fs_read
cargo test -p agentdash-application provider_inline
cargo test -p agentdash-application provider_skill_asset
cargo test -p agentdash-executor anthropic
git diff --check
```
