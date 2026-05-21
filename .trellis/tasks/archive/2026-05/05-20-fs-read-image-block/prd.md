# fs_read 图片 Block 返回能力

## Goal

让 Agent 通过文件读取工具读取图片资产时，能够收到模型可消费的图片 Block，而不是只能得到文本错误或路径描述。该能力应建立在 VFS 的二进制读取契约之上，并与现有 `ContentPart::Image` / connector bridge 语义对齐。

## Problem

当前 session / agent 类型系统已经存在图片表达：

- tool result 支持 `Vec<ContentPart>`。
- `ContentPart::Image { mime_type, data }` 已存在。
- 部分 connector bridge 能把图片内容转换给模型。

但 `fs_read` 当前仍然只调用 VFS `read_text`，因此即使 `inline_fs` 已能存储图片，Agent 也不能用标准读取工具把图片作为图片输入拿到。

## Requirements

- 明确 `ContentPart::Image.data` 的规范：base64 bytes、data URL，或分字段表示；connector bridge 必须统一处理。
- VFS / Relay 层提供二进制读取能力，至少能读取 image MIME 文件。
- `fs_read` 在目标文件为 `image/*` 时返回图片 Block，并附带简短文本说明或 metadata。
- `fs_read` 对非图片 binary 返回清晰错误，避免把任意二进制塞进模型上下文。
- 读取行为应基于 stat/list metadata 或 provider 能力判断，不靠文件扩展名硬猜。
- `read_text` 的现有文本语义保持清晰：文本文件仍按原逻辑返回带行号内容。
- 支持 `inline_fs` 图片读取；后续可自然扩展到 `skill_asset_fs` 图片。

## Non-Goals

- 不在本任务里实现 Skill asset 图片上传；那属于 `skill-asset-binary-content`。
- 不在本任务里实现图片生成 Agent 的临时 artifact registry。
- 不提供通用 bash 或任意文件系统二进制读取接口。

## Acceptance Criteria

- [x] `fs_read` 读取 `inline_fs` 图片文件时，tool result 包含 `ContentPart::Image`。
- [x] 图片 Block 在至少一个现有 connector bridge 中格式正确，不发生 data URL/base64 语义错配。
- [x] `fs_read` 读取文本文件的行为和测试不回退。
- [x] `fs_read` 读取非图片 binary 时返回明确的 unsupported/error 文本结果。
- [x] VFS provider / relay 的 binary read contract 有单元测试覆盖。
- [x] 工具层测试覆盖 image result、text result、unsupported binary 三类路径。

## Execution Notes

- 本任务需要先做一小段 research：梳理 `ContentPart::Image` 到 Anthropic / Codex connector 的实际期望格式。
- 建议在 design.md 中定下 `Image.data` 的 canonical representation，再改 tool 与 bridge，避免后续 connector 分叉。
