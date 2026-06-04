# [child-2] 图片输入端到端（采集→投递→ContentPart::Image）

Parent: [06-04-session-composer-redesign-image-input](../06-04-session-composer-redesign-image-input/prd.md)
**依赖：child-1（[06-04-userinput-canonical-path]）必须先完成** —— 否则图片仍被拍平成文本，模型看不到真图。

## Goal

让用户从 composer 用**剪贴板粘贴 + 拖拽**加入图片，端到端送达模型为真正的图像（`ContentPart::Image`），含预览/删除/约束。

## Requirements

- R1 采集：剪贴板粘贴（Ctrl+V）+ 拖拽放入 + **child-3「+」菜单拉起的本地文件/图片 picker**（三入口共用同一图片附件管线）；仅 `image/*`；非图片不拦截。约束：单图 ≤5MB、单次 ≤6 张，超限有可见提示不静默丢弃。
- R2 预览管理：缩略图预览 + 单张删除；发成功清空、发失败保留。
- R3 投递：前端用 child-1 的统一 `UserInputBlock` builder 产出 `[text?, image...]`；图片以 data URL（`data:<mime>;base64,<data>`）形式进入 `UserInputBlock::Image`。send 与 steer 共用同一 builder（child-1 已统一入参）。
- R4 端到端验证：抓一次 provider 请求/日志，确认模型收到 image content part，而非 `"[引用图片: ...]"` 占位文本。

## Acceptance Criteria

- [ ] 粘贴/拖拽图片出现预览，可删除，超限有提示。
- [ ] 有图无字可发送（依赖 child-3 的可用性判定或本 child 自带）。
- [ ] 端到端：模型实际收到图片（`ContentPart::Image`）；steer 注图同样生效（执行器支持时）。
- [ ] `pnpm -F app-web lint/typecheck/test` 通过；采集/约束/builder 有单测。

## 范围外

- composer 整体视觉重构（child-3，本 child 只接入采集与预览所需的最小 UI 钩子）。
- 非图片附件、对象存储（程序级范围外）。（文件/图片 picker 入口由 child-3「+」提供，本 child 只接其图片产物。）

## Notes

- 采集 hook、约束常量、`FileReader.readAsDataURL` 拆 mime/base64 的细节，design.md 在本 child 激活时补。
- 与 child-3 的协作：预览区/附件入口最终落到 child-3 的新布局；若 child-3 先行，则本 child 复用其槽位。
