# [Parent] 集成与排期跟踪

父任务不实现具体代码；只跟踪 child 顺序、跨 child 集成点与最终验收。父任务保留 planning，直到 4 个 child 全部 archive 再做集成 review。

## 执行顺序

1. **child-1 userinput-canonical-path**（硬前置）→ 完成并 archive 后再开下一个。
2. **child-2 image-input-e2e**（依赖 child-1）。
3. **child-3 composer-redesign-model-selector**（建议 child-1 后；与 child-4 UI 协同）。
4. **child-4 dual-mode-append-messaging**（依赖 child-1；与 child-3 协同）。

> child-2 与 child-3 在 child-1 完成后理论可并行；单人推进建议串行：child-1 → child-2 → child-3 → child-4。

## 跨 child 集成检查点

- child-1 完成时：后端 + 既有前端发送路径在新 canonical 契约下编译/测试通过（前端即便还没重构，也要跑通发送）。
- child-2 完成时：端到端图片真达模型（抓一次 provider 请求/日志确认是 image 而非占位文本）。
- child-3 完成时：composer 重构不回归 @ 引用/token/steer/send/cancel/promptTemplates。
- child-4 完成时：排队/steer 双模 + capability 退化 + 与 child-3 UI 协同无冲突。

## 最终集成 review（父任务收尾）

- [ ] 跨 child 验收项（见 parent prd.md）全绿。
- [ ] 全仓 `lint`/`typecheck`/`test`（app-web + 后端相关 crate）。
- [ ] spec 更新：把 canonical 输入路径写进 .trellis/spec（backend session / cross-layer）。
- [ ] 4 child 均 archive 后再归档父任务。
