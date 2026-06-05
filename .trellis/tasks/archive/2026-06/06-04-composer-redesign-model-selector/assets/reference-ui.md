# Composer 参考 UI（用户提供截图）

本目录原图（文件名→内容）：
- `composer-empty.png` — **空状态 composer**：`+ Send follow-up` 占位，右侧「Opus 4.6 Max ⌄」。
- `composer-filled.png` — **有内容 composer**：文本「测试」+ 文件药丸「TS lifecycleStore.ts 引用文件」+「/trellis-start 引用Skill，引用指令」；左下「+」与「Opus 4.6 Max ⌄」，右下发送上箭头。
- `model-reasoning-popup.png` — **模型/推理两级下拉**（展开态）。
- 已排队消息行图归 child-4：[../../06-04-dual-mode-append-messaging/assets/queued-message-row.png](../../06-04-dual-mode-append-messaging/assets/queued-message-row.png)，转录见 [child-4 reference-ui.md](../../06-04-dual-mode-append-messaging/assets/reference-ui.md)。

> 注：截图里出现的**麦克风图标统一不要**（无语音输入）。发送区只保留一个**上箭头发送按钮**，执行中变为**停止方块**（见下"发送按钮形态"）。

## 整体 composer

- **左侧「+」**：会话快捷操作的**通用菜单栏**入口（GUI），可扩展。MVP 先放一行「拉起**引用本地文件/图片**的选择器」（即拉起文件/图片 picker），其余快捷项后续补。
  - 选文件 → 走既有 `@` 文件引用（`Mention`）；选图片 → 喂 child-2 的图片附件管线（与粘贴/拖拽同一入口）。
  - **注意**：这里**不是**模型选择器。模型/推理选择是**左下角的独立 chip**（见下）。
- **左下**：紧凑模型 chip「Opus 4.6 Max ⌄」/「5.5 高 ⌄」，点开 = 下方两级下拉。
- **右下**：单个**上箭头发送按钮**（深色圆形）；执行中显示为**■ 停止方块**。**不要麦克风。**
- 输入区支持 slash 命令、@文件引用药丸、引用 Skill/指令 混排。

## 模型 / 推理两级下拉（model-reasoning-popup.png）

> ⚠️ **仅参考样式**：图中的 GPT-5.5 / 速度 / 推理档项都是示意。实际**模型与档位以项目真实 discovered options 为准**，且 **必须按 provider 分组**（左列分组入口 = provider，右列 = 该 provider 下的模型）。不要硬编码截图里的型号。

```
┌─ Reasoning ──────┐   ┌─ <Provider A> ───────┐
│ Off              │   │ <model 1>      ✓     │
│ Minimal          │   │ <model 2>            │
│ Low              │   │ <model 3>            │
│ Medium           │   └──────────────────────┘
│ High         ✓   │
│ Xhigh            │
│ ──────────────── │
│ <Provider A>  ▸ │  ← provider 分组入口，▸ 展开右列模型
│ <Provider B>  ▸ │
└──────────────────┘
底部 chip：「<model> High ⌄」
```

- 左列上半 = **Reasoning（推理档）**：取项目实际暴露集合 `THINKING_LEVEL_OPTIONS`（[packages/app-web/src/types/index.ts](../../../packages/app-web/src/types/index.ts)）= **Off / Minimal / Low / Medium / High / Xhigh**（+「默认不指定」空值）。**含 Off（关推理）**。← `execConfig.thinkingLevel`。
- 左列下半 = **provider 分组入口**（实际 provider，非截图的 GPT-5.5/速度），▸ 展开右列。
- 右列 = **该 provider 下的模型列表**。← discovered `model_selector.models`，按 `provider_id` 分组。
- chip 文案 = `<模型简称> <Reasoning>`。
- 选择需同时落 `providerId` + `modelId`。
- **统一英文标签**；**不要「速度」那一项**（截图里的 `速度 ▸` 是无关示意，本项目不实现）。
- `showThinkingSelector` 现有逻辑：仅选中模型 `reasoning===true`（或未选模型）时显示推理档，沿用。

## 发送按钮形态（用户指定）

单按钮，按执行/输入状态切换：

| 状态 | 形态 | 点击语义 |
|---|---|---|
| idle | ↑ 发送 | 发送（新 turn） |
| running + 输入框为空 | ■ 停止 | cancel 当前 turn |
| running + 输入框有新内容 | ↑ 发送 | 发送（默认=排队，详见 child-4） |

→ 取代现有"发送/取消"右侧竖排双按钮，合并为单个 morphing 按钮。

## 映射到现有能力

- 推理档 ← `execConfig.thinkingLevel`（low/medium/high/超高，需对齐现有枚举）。
- 模型列表 ← `useExecutorDiscoveredOptions().model_selector.models`。
- 仍需容纳 provider / permission / reset / reconnect（现 ExecutorSelector 能力）：放下拉次级区或"更多"，不在主 chip 暴露。
- 键盘与发送语义的权威定义在 child-4（Enter/Ctrl+Enter/Shift+Enter）。
- steer 态：模型/推理 chip 只读（child-4 协同）。
