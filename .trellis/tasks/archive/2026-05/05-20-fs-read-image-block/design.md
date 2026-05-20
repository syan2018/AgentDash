# fs_read 图片 Block 返回能力设计

## ContentPart 图片规范

`ContentPart::Image { mime_type, data }` 的 canonical representation 是：

- `mime_type`: 原始 MIME，如 `image/png`
- `data`: 原始文件 bytes 的 standard base64 字符串，不包含 `data:` URL 前缀

理由：

- Anthropic Messages API 的 image block 使用 `source.type = "base64"`，需要纯 base64。
- 前端 ACP 渲染路径已经按 `data:${mimeType};base64,${data}` 组装。
- session continuation 的 `ContentBlock::Image` 到 `ContentPart::Image` 映射沿用同一语义。

Codex protocol 的 `InputImage.image_url` 需要可消费 URL，因此 bridge/UI 映射到 Codex app server protocol 时在边界处从 `mime_type + base64` 组装 data URL。

## VFS 二进制读取契约

在 `MountProvider` trait 增加只读方法：

```rust
async fn read_binary(...) -> Result<BinaryReadResult, MountError>
```

默认实现返回 `NotSupported`。Provider 能力仍由 `Read` 控制；`read_binary` 只是 `read` 能力下的 typed content channel，不新增 mount capability。

`BinaryReadResult` 包含：

- `path`
- `bytes`
- `mime_type`
- `attributes`

`inline_fs` 与 `skill_asset_fs` 实现该方法；文本文件读取二进制返回 `NotSupported`。其它 provider 先保持默认实现。

## fs_read 行为

`fs_read` 先 `stat`，按 `attributes.content_kind` 判断内容类型：

- 非 binary 或缺少 content_kind：沿用现有 `read_text` + 行号输出。
- binary + `mime_type` 以 `image/` 开头：调用 `read_binary`，base64 编码后返回两个 content part：
  - `Text`: path、mime、size metadata
  - `Image`: `mime_type` + base64 data
- binary + 非 image MIME：返回 `is_error = true` 的文本结果，说明该二进制类型不支持直接读入模型。

行号参数只适用于文本文件；图片结果忽略 `start_line` / `end_line`。

## Connector 对齐

- Anthropic user image block 继续直接使用 base64。
- Anthropic tool_result 需要保留 image part，而不是只拼文本。
- Codex app protocol 的动态工具结果 image item 使用 data URL，避免把纯 base64 塞到 `image_url`。

OpenAI Codex Responses 当前 function tool output 仍是文本输出；本任务不扩展该 connector 的函数结果图片输入。
