# Session artifact 资产归档与维护能力

## Goal

规划一套面向 Agent 生成资产的受控管理能力，让会话中产生的图片等 artifact 默认先进入会话临时资产区，并可由用户或主 Agent 选择性归档、复制、重命名、移动到 `inline_fs` 或后续资产库。

该任务承接 `inline_fs 图片资产支持` 的长期讨论，不阻塞当前 `inline_fs` 图片存储与预览主线。

## Confirmed Direction

- 图像生成 Agent 或 subagent 的输出默认由 session runtime 记录为临时 artifact。
- 临时 artifact 不应默认直接写入长期 `inline_fs`，避免试错产物污染上下文资产目录。
- 长期能力应优先设计受控 asset/VFS tools，而不是直接把通用 bash 作为唯一资产维护入口。
- `inline_fs` 图片资产任务只负责成为可保存、可预览、可引用的目标端。

## Discussion Topics

- Session artifact 的领域模型：`artifact_id`、`mime_type`、`size_bytes`、source agent、tool call、turn、临时 blob 存储位置。
- Promote 流程：从 `artifact://...` 归档到 `inline_fs://...` 或 `mount_id://...`。
- 冲突策略：fail / overwrite / auto_rename / version。
- Provenance：归档后是否保留原始生成信息、prompt、模型、agent、tool call。
- 资产操作工具：`assets.promote_artifact`、`assets.copy`、`assets.move`、`assets.rename`、`assets.delete`、`assets.list`。
- Minimal bash / restricted job runner 的边界：什么时候需要 shell，什么时候必须走平台 asset API。
- 权限、审计、回滚和清理策略。

## Initial Recommendation

优先设计 “session artifact registry + asset promotion tools”：

1. 生成产物进入会话临时资产区。
2. 用户或主 Agent 通过受控工具 promote 到目标资产位置。
3. 平台负责权限、命名、冲突、provenance 和 VFS 写入。
4. 后续再评估是否需要受限 bash 或 job runner 处理批量转换、压缩、重命名。

## Acceptance Criteria For Future Planning

- [ ] 明确 session artifact 的生命周期和清理策略。
- [ ] 明确 promote 到 `inline_fs` / 资产库的 API 和权限模型。
- [ ] 明确主 Agent 可以执行哪些资产维护动作。
- [ ] 明确 minimal bash 是否需要，以及它与 VFS/asset tools 的边界。
- [ ] 形成可实施的 design.md / implement.md 后再进入实现。
