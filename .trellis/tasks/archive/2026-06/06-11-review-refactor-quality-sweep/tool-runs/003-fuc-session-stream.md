# 🌸 屎山代码分析报告 🌸

## 📑 目录

- [糟糕指数](#overall-score)
- [评分指标详情](#metrics-details)
- [最屎代码排行榜](#problem-files)
- [诊断结论](#conclusion)

![Score](https://img.shields.io/badge/Score-91%25-brightgreen)

## 糟糕指数 {#overall-score}

| 指标摘要 | 评分 |
|------|-------|
| **糟糕指数** | **90.70/100** |
| 屎山等级 | 🌸 偶有异味 |

> 如沐春风，仿佛被天使亲吻过

### 📊 统计信息

| 指标 | 数值 |
|--------|-------|
| 总文件数 | 65 |
| 已跳过 | 12 |
| 耗时 | 447ms |

## 评分指标详情 {#metrics-details}

| 指标摘要 | 评分 | 状态 |
|:-----|------:|:------:|
| 循环复杂度 | 7.88% | ✓✓ |
| 认知复杂度 | 7.25% | ✓✓ |
| 嵌套深度 | 1.92% | ✓✓ |
| 函数长度 | 1.36% | ✓✓ |
| 文件长度 | 3.60% | ✓✓ |
| 参数数量 | 0.00% | ✓✓ |
| 代码重复 | 0.89% | ✓✓ |
| 结构分析 | 1.18% | ✓✓ |
| 错误处理 | 5.79% | ✓✓ |
| 注释比例 | 39.73% | ○ |
| 命名规范 | 0.00% | ✓✓ |

## 最屎代码排行榜 {#problem-files}

### 1. model\useSessionStream.ts

**糟糕指数: 24.50**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 2, 🏗️ 结构问题: 1, ❌ 错误处理问题: 4, 📝 注释问题: 1

- 🔄 `applyEventToEntries()` L164: 复杂度: 34
- 🔄 `applyEventToEntries()` L164: 认知复杂度: 40
- 📏 `applyEventToEntries()` L164: 179 代码量
- 🏗️ `applyEventToEntries()` L164: 中等嵌套: 3
- ❌ L557: 未处理的易出错调用
- 🔍 ...还有 3 个问题实在太屎，列不完了

### 2. ui\contextFrame\SectionRenderers.tsx

**糟糕指数: 24.22**

**问题**: 🔄 复杂度问题: 3, ⚠️ 其他问题: 2, 🏗️ 结构问题: 1, 📝 注释问题: 1

- 🔄 `SectionBlock()` L43: 复杂度: 25
- 🔄 `renderSectionBody()` L159: 复杂度: 12
- 🔄 `SectionBlock()` L43: 认知复杂度: 27
- 📏 `SectionBlock()` L43: 114 代码量

### 3. ui\ContextFrameStream.tsx

**糟糕指数: 22.06**

**问题**: 🔄 复杂度问题: 5, 🏗️ 结构问题: 2

- 🔄 `summarizeRuntimeUpdate()` L185: 复杂度: 12
- 🔄 `frameTabLabel()` L160: 认知复杂度: 16
- 🔄 `summarizeRuntimeUpdate()` L185: 认知复杂度: 26
- 🔄 `frameTabLabel()` L160: 嵌套深度: 4
- 🔄 `summarizeRuntimeUpdate()` L185: 嵌套深度: 7
- 🔍 ...还有 2 个问题实在太屎，列不完了

### 4. ui\toolCardRegistry.ts

**糟糕指数: 22.00**

**问题**: 🔄 复杂度问题: 5, ⚠️ 其他问题: 2, 🏗️ 结构问题: 1, 📝 注释问题: 1

- 🔄 `renderToolCallCard()` L41: 复杂度: 19
- 🔄 `getDynamicToolHeader()` L195: 复杂度: 28
- 🔄 `renderToolCallCard()` L41: 认知复杂度: 21
- 🔄 `getDynamicToolHeader()` L195: 认知复杂度: 32
- 🔄 `sumDiffStats()` L295: 嵌套深度: 4
- 🔍 ...还有 2 个问题实在太屎，列不完了

### 5. ui\SessionSystemEventCard.tsx

**糟糕指数: 19.64**

**问题**: 🔄 复杂度问题: 5, ⚠️ 其他问题: 1, 🏗️ 结构问题: 1, ❌ 错误处理问题: 1, 📝 注释问题: 1

- 🔄 `buildHookExpandData()` L430: 复杂度: 13
- 🔄 `extractHookEventDataFromTrace()` L597: 复杂度: 11
- 🔄 `extractHookEventDataFromRecord()` L655: 复杂度: 22
- 🔄 `buildHookExpandData()` L430: 认知复杂度: 17
- 🔄 `extractHookEventDataFromRecord()` L655: 认知复杂度: 26
- 🔍 ...还有 1 个问题实在太屎，列不完了

### 6. model\contextFrame.ts

**糟糕指数: 18.44**

**问题**: 🔄 复杂度问题: 4, ⚠️ 其他问题: 2, 📋 重复问题: 1, 🏗️ 结构问题: 1, 📝 注释问题: 1

- 🔄 `parseSection()` L222: 复杂度: 26
- 🔄 `sectionKindToToken()` L504: 复杂度: 16
- 🔄 `parseSection()` L222: 认知复杂度: 28
- 🔄 `sectionKindToToken()` L504: 认知复杂度: 18
- 📏 `parseSection()` L222: 147 代码量
- 🔍 ...还有 1 个问题实在太屎，列不完了

### 7. ui\ContentBlockCard.tsx

**糟糕指数: 15.28**

**问题**: 🔄 复杂度问题: 4, 📝 注释问题: 1

- 🔄 `getMimeTypeBadge()` L15: 复杂度: 15
- 🔄 `getFileBadgeFromName()` L37: 复杂度: 26
- 🔄 `getMimeTypeBadge()` L15: 认知复杂度: 19
- 🔄 `getFileBadgeFromName()` L37: 认知复杂度: 28

### 8. ui\bodies\readPayload.ts

**糟糕指数: 15.17**

**问题**: 🔄 复杂度问题: 3, 🏗️ 结构问题: 1

- 🔄 `parseReadToolText()` L19: 复杂度: 13
- 🔄 `parseReadToolText()` L19: 认知复杂度: 21
- 🔄 `parseReadToolText()` L19: 嵌套深度: 4
- 🏗️ `parseReadToolText()` L19: 中等嵌套: 4

### 9. model\useSessionFeed.ts

**糟糕指数: 10.87**

**问题**: 🔄 复杂度问题: 4, ⚠️ 其他问题: 1, 📋 重复问题: 1, 🏗️ 结构问题: 2, 📝 注释问题: 1

- 🔄 `getToolAggregationType()` L57: 复杂度: 13
- 🔄 `classifyEntry()` L115: 复杂度: 11
- 🔄 `isAggregatedGroupEqual()` L327: 复杂度: 21
- 🔄 `isAggregatedGroupEqual()` L327: 认知复杂度: 27
- 📋 `pushToolGroup()` L185: 重复模式: pushToolGroup, pushCtxSideGroup
- 🔍 ...还有 2 个问题实在太屎，列不完了

### 10. ui\SessionEntry.tsx

**糟糕指数: 10.47**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 2, ❌ 错误处理问题: 2, 📝 注释问题: 1

- 🔄 `SingleEntry()` L71: 复杂度: 13
- 🔄 `SingleEntry()` L71: 认知复杂度: 17
- 📏 `SingleEntry()` L71: 107 代码量
- ❌ L296: 未处理的易出错调用
- ❌ L301: 未处理的易出错调用

### 11. ui\bodies\toolOutputContent.ts

**糟糕指数: 9.86**

**问题**: 🔄 复杂度问题: 2, 🏗️ 结构问题: 1, 📝 注释问题: 1

- 🔄 `normalizeMcpOutput()` L45: 复杂度: 17
- 🔄 `normalizeMcpOutput()` L45: 认知复杂度: 23
- 🏗️ `normalizeMcpOutput()` L45: 中等嵌套: 3

### 12. model\streamTransport.ts

**糟糕指数: 9.14**

**问题**: 🏗️ 结构问题: 2, ❌ 错误处理问题: 4, 📝 注释问题: 1

- 🏗️ `consumeStream()` L175: 中等嵌套: 3
- 🏗️ `handleLine()` L202: 中等嵌套: 3
- ❌ L127: 未处理的易出错调用
- ❌ L130: 未处理的易出错调用
- ❌ L249: 未处理的易出错调用
- 🔍 ...还有 1 个问题实在太屎，列不完了

### 13. model\types.ts

**糟糕指数: 8.56**

**问题**: 🔄 复杂度问题: 6, ⚠️ 其他问题: 1, 🏗️ 结构问题: 1, 📝 注释问题: 1

- 🔄 `parseContentBlock()` L125: 复杂度: 16
- 🔄 `extractTextFromContentBlock()` L193: 复杂度: 13
- 🔄 `getThreadItemTitle()` L420: 复杂度: 16
- 🔄 `parseContentBlock()` L125: 认知复杂度: 20
- 🔄 `extractTextFromContentBlock()` L193: 认知复杂度: 17
- 🔍 ...还有 1 个问题实在太屎，列不完了

### 14. model\threadItemKind.ts

**糟糕指数: 8.16**

**问题**: 🔄 复杂度问题: 2, 📝 注释问题: 1

- 🔄 `resolveKind()` L60: 复杂度: 13
- 🔄 `resolveDynamicKind()` L78: 复杂度: 12

### 15. ui\SessionProjectionView.tsx

**糟糕指数: 7.70**

**问题**: ⚠️ 其他问题: 1, 📋 重复问题: 1, 📝 注释问题: 1

- 📋 `originLabel()` L48: 重复模式: originLabel, roleLabel

## 诊断结论 {#conclusion}

🌸 **偶有异味** - 基本没事，但是有伤风化

👍 继续保持，你是编码界的一股清流，代码洁癖者的骄傲

---

*由 [fuck-u-code](https://github.com/Done-0/fuck-u-code) 生成*