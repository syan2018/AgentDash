# Skill asset 二进制资产存储收敛

## Goal

让 Skill 上传、远端导入、VFS 投影和资产预览能够正确处理图片等二进制文件，并把 `skill_asset_files` 与 `inline_fs_files` 当前已经形成的 text/binary 内容模型收敛为共享能力，避免两个领域重复实现二进制内容存储、metadata、读写边界和搜索行为。

## Problem

当前 Skill asset 仍然是 text-only：

- `SkillAssetFile.content` 是 `String`。
- `skill_asset_files.content` 是 `TEXT NOT NULL`。
- 上传 zip 或 multipart 时，非 `SKILL.md` 文件也会被强制 UTF-8 解码。
- `skill_asset_fs` VFS projection 使用 `BTreeMap<String, String>`，只能投影文本内容。

这会导致带图片的 Skill 上传失败，也会让未来插件/Skill 资产与 `inline_fs` 的图片资产能力分叉。

## Requirements

- Skill asset 文件内容支持 text / binary 两种内容类型。
- `SKILL.md` 必须继续作为 UTF-8 文本解析 metadata；图片等资源文件不得被强制文本解码。
- `skill_asset_files` 迁移到带 `content_kind`、`mime_type`、`text_content`、`binary_content`、`size_bytes` 的正确结构。
- 尽量抽出共享内容模型或共享 helper，使 `inline_fs` 与 `skill_asset` 复用二进制内容表达、size/mime metadata、text-only search 边界。
- Skill 上传路径支持图片文件：zip entry 和普通上传都按 bytes 进入应用层，再由路径/mime 判断保存形式。
- `skill_asset_fs` list/stat 暴露 `content_kind`、`mime_type`、`size`；`read_text` 对 binary 返回明确错误；`search_text` 跳过 binary。
- Skill asset 的业务约束保持独立：路径映射、`SKILL.md` metadata 解析、文件 kind 推断、主文档删除/重命名限制。
- 不引入 base64/data-url 文本存储作为长期方案。

## Non-Goals

- 不在本任务里实现 Agent 通过 tool 读取图片 Block。
- 不在本任务里设计 session 临时 artifact registry 或 promote 流程。
- 不保留 text-only Skill asset 的兼容分支；项目仍处于预研期，迁移后采用正确结构。

## Acceptance Criteria

- [ ] 上传包含图片资源的 Skill 不再因非 UTF-8 内容失败。
- [ ] Skill asset 的图片文件能被持久化、列表展示，并带有正确 `content_kind` / `mime_type` / `size_bytes`。
- [ ] `SKILL.md` metadata 校验仍然只基于文本主文档，且错误信息清晰。
- [ ] `skill_asset_fs` 文本读写、搜索、删除、重命名仍通过现有测试。
- [ ] binary 文件不会进入 text search，也不会被 `read_text` 当文本返回。
- [ ] `inline_fs` 与 `skill_asset` 的共享内容模型或 helper 已落在合适层级，避免重复实现。
- [ ] 覆盖上传图片、仓储读写、VFS metadata、text/binary 边界的后端测试。

## Execution Notes

- 本任务建议先做 design.md，再实现；重点是选定共享内容模型放置层级，避免 domain 间反向依赖。
- 可优先从 `InlineFileContent` 的形状抽象，而不是让 Skill asset 直接依赖 inline_fs domain entity。
