# 模块：信息注入（Injection）

## 核心思想

注入是**策略组合**，不是固定管道。

我们不预设"必须读哪些文件"或"按什么顺序"，因为：
- 不同项目有不同的信息源（设计文档、规范、历史经验）
- 不同Task需要不同上下文（编码Task vs 设计Task）
- 信息量需要权衡（太少不够用，太多会淹没）

系统只保证：**执行需要上下文支撑**，但"如何注入"可自由组合。

## 定位

将设计信息、上下文、规范等注入到Story和Task中，为执行提供必要的信息支撑。

## 职责

- 支持多种信息源的注入（文档、模板、动态上下文）
- 定义注入的时机和范围
- 管理信息的继承和覆盖关系
- 提供注入内容的验证机制

## 核心概念

### 注入源（Injection Source）
- 设计文档（PRD、需求规格等）
- 规范文件（编码规范、设计规范等）
- 项目上下文（项目结构、依赖关系等）
- 历史记录（相似Story的参考）

### 注入点（Injection Point）
- Story创建时（初始化上下文）
- Task创建时（从Story继承+特定注入）
- Task执行前（实时上下文更新）

### 注入策略（Injection Strategy）
- 合并：多个源的内容合并后注入
- 覆盖：后注入的内容覆盖前者
- 追加：在已有内容后追加新内容

## 注入流程

```
识别注入源
    ↓
解析内容（根据文件类型）
    ↓
应用注入策略（合并/覆盖/追加）
    ↓
构建注入上下文
    ↓
注入到目标（Story/Task）
    ↓
验证注入完整性
```

## 接口定义（概念层面）

```
Injector {
  injectToStory(storyId, sources): Context
  injectToTask(taskId, sources): Context
  defineSource(name, type, location): Source
  getAvailableSources(): Source[]
}

Source {
  id: string
  name: string
  type: "document" | "template" | "context"
  location: string
  format: "markdown" | "json" | "yaml" | "auto"
}

Injection {
  targetType: "story" | "task"
  targetId: string
  sources: string[]
  strategy: "merge" | "override" | "append"
  result: Context
}
```

## 关键设计决策（待讨论）

- [ ] 注入源的定义格式
- [ ] 内容解析器的可扩展性
- [ ] 大文件/大量内容的处理策略
- [ ] 注入结果的缓存机制

## 暂不定义

- 具体文件解析实现
- 模板引擎选择
- 内容版本管理
- 敏感信息过滤

---

*状态：概念定义阶段*  
*更新：2026-02-21*
