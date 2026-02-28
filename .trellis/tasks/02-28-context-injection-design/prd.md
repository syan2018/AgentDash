# 上下文注入机制（Injection 模块）

## 背景与目标

根据设计文档 `docs/modules/06-injection.md`，Injection 模块负责将设计信息、上下文、规范等注入到 Story 和 Task 中，为 Agent 执行提供必要的信息支撑。

当前状态：
- 概念设计已完成（注入源、注入点、注入策略）
- 尚无具体实现

本任务要建立可扩展的上下文注入框架，为后续的 "@引用文件" 等功能提供基础能力。

## 目标

1. 建立 `Injector` trait 框架，支持多种注入源
2. 实现核心注入源：设计文档、规范文件、项目上下文
3. 定义注入内容的格式和合并策略
4. 为 Task-Agent 绑定提供上下文注入能力

## 非目标

- 不实现具体文件解析（如 PDF、Word 等）
- 不实现模板引擎
- 不做智能内容筛选（避免"上下文过多"问题暂时通过用户选择解决）

## 核心概念（来自设计文档）

### 注入源（Injection Source）

- 设计文档（PRD、需求规格等）
- 规范文件（编码规范、设计规范等）
- 项目上下文（项目结构、依赖关系等）
- 历史记录（相似 Story 的参考）
- 远端文件（Git 历史、URL、请求KM文档 等）

### 注入点（Injection Point）

- Story 创建时（初始化上下文）
- Task 创建时（从 Story 继承+特定注入）
- Task 执行前（实时上下文更新）

### 注入策略（Injection Strategy）

- 合并：多个源的内容合并后注入
- 覆盖：后注入的内容覆盖前者
- 追加：在已有内容后追加新内容

## 需求规格

### FR-1: Injector Trait 框架

定义核心 trait：

```rust
/// 注入器接口
pub trait Injector: Send + Sync {
    /// 注入器名称
    fn name(&self) -> &str;

    /// 执行注入
    fn inject(&self, ctx: &mut InjectionContext, sources: &[SourceRef]) -> Result<InjectionResult, InjectionError>;
}

/// 注入上下文
pub struct InjectionContext {
    /// 目标类型
    pub target_type: InjectionTarget,
    /// 目标 ID
    pub target_id: String,
    /// 当前已注入的内容
    pub current_content: String,
    /// 工作空间路径（用于解析相对路径）
    pub workspace_path: Option<PathBuf>,
}

pub enum InjectionTarget {
    Story,
    Task,
    Session,
}

/// 注入源引用
pub struct SourceRef {
    /// 源类型
    pub source_type: SourceType,
    /// 源位置（文件路径、URL 等）
    pub location: String,
    /// 注入权重（用于排序）
    pub priority: i32,
}

pub enum SourceType {
    /// 设计文档
    DesignDoc,
    /// 规范文件
    SpecFile,
    /// 项目上下文
    ProjectContext,
    /// 历史记录
    History,
    /// 自定义源
    Custom(String),
}

/// 注入结果
pub struct InjectionResult {
    /// 最终注入内容
    pub content: String,
    /// 实际使用的源
    pub applied_sources: Vec<SourceRef>,
    /// 统计信息
    pub stats: InjectionStats,
}

pub struct InjectionStats {
    pub total_chars: usize,
    pub source_count: usize,
}
```

### FR-2: 内置注入器实现

#### 2.1 文件注入器（FileInjector）

支持从文件系统读取内容：

```rust
pub struct FileInjector;

impl Injector for FileInjector {
    fn inject(&self, ctx: &mut InjectionContext, sources: &[SourceRef]) -> Result<InjectionResult, InjectionError> {
        // 读取文件内容，支持格式：
        // - Markdown (.md)
        // - 纯文本 (.txt)
        // - JSON (.json) - 格式化后注入
        // - YAML (.yaml/.yml) - 格式化后注入
    }
}
```

#### 2.2 项目上下文注入器（ProjectContextInjector）

自动收集项目信息：

```rust
pub struct ProjectContextInjector;

impl Injector for ProjectInjector {
    fn inject(&self, ctx: &mut InjectionContext, _sources: &[SourceRef]) -> Result<InjectionResult, InjectionError> {
        // 收集：
        // - 项目结构（目录树，排除 node_modules/target 等）
        // - 技术栈检测（package.json -> Node.js, Cargo.toml -> Rust 等）
        // - 关键配置文件（README、CLAUDE.md 等）
    }
}
```

#### 2.3 复合注入器（CompositeInjector）

支持多种策略组合多个注入器：

```rust
pub enum MergeStrategy {
    /// 按优先级排序后连接
    Concat,
    /// 智能合并（去重相同内容）
    SmartMerge,
    /// 后者覆盖前者
    Override,
}

pub struct CompositeInjector {
    injectors: Vec<(Box<dyn Injector>, MergeStrategy)>,
}
```

### FR-3: 内容格式化

定义统一的注入内容格式，便于 Agent 理解：

```markdown
# 上下文注入

## 来源: {source_name}
{content}

---

## 来源: {source_name}
{content}
```

对于结构化内容（JSON/YAML），转换为 Markdown 格式：

```rust
fn format_as_markdown(content: &str, format: ContentFormat) -> String {
    match format {
        ContentFormat::Json => json_to_markdown(content),
        ContentFormat::Yaml => yaml_to_markdown(content),
        ContentFormat::Markdown => content.to_string(),
        ContentFormat::PlainText => content.to_string(),
    }
}
```

### FR-4: 注册与管理

注入器注册表：

```rust
pub struct InjectorRegistry {
    injectors: HashMap<String, Box<dyn Injector>>,
}

impl InjectorRegistry {
    pub fn register(&mut self, name: &str, injector: Box<dyn Injector>);
    pub fn get(&self, name: &str) -> Option<&dyn Injector>;
    pub fn create_pipeline(&self, names: &[&str]) -> CompositeInjector;
}
```

### FR-5: 与 Task 集成

为 Task 提供便捷的上下文注入接口：

```rust
impl Task {
    /// 为 Task 构建执行上下文
    pub async fn build_execution_context(
        &self,
        registry: &InjectorRegistry,
        workspace: &Workspace,
    ) -> Result<String, InjectionError> {
        let mut ctx = InjectionContext {
            target_type: InjectionTarget::Task,
            target_id: self.id.to_string(),
            current_content: String::new(),
            workspace_path: Some(workspace.path.clone()),
        };

        // 1. 注入 Story 上下文
        if let Some(story_context) = &self.story_context {
            ctx.current_content.push_str(&story_context);
        }

        // 2. 注入 Task 特定配置
        if let Some(sources) = &self.injection_sources {
            let injector = registry.create_pipeline(&["file", "project_context"]);
            let result = injector.inject(&mut ctx, sources)?;
            ctx.current_content = result.content;
        }

        Ok(ctx.current_content)
    }
}
```

## 技术方案

### 模块结构

```
crates/agentdash-injection/
├── src/
│   ├── lib.rs
│   ├── traits.rs          # Injector trait 定义
│   ├── context.rs         # InjectionContext 等类型
│   ├── registry.rs        # InjectorRegistry
│   ├── injectors/
│   │   ├── mod.rs
│   │   ├── file.rs        # FileInjector
│   │   ├── project.rs     # ProjectContextInjector
│   │   └── composite.rs   # CompositeInjector
│   ├── formatters/
│   │   ├── mod.rs
│   │   ├── markdown.rs    # Markdown 格式化
│   │   ├── json.rs        # JSON 转 Markdown
│   │   └── yaml.rs        # YAML 转 Markdown
│   └── error.rs
└── Cargo.toml
```

### 使用示例

```rust
// 初始化注册表
let mut registry = InjectorRegistry::new();
registry.register("file", Box::new(FileInjector));
registry.register("project", Box::new(ProjectContextInjector));

// 构建注入管道
let pipeline = registry.create_pipeline(&["project", "file"]);

// 执行注入
let mut ctx = InjectionContext {
    target_type: InjectionTarget::Task,
    target_id: task.id.to_string(),
    current_content: String::new(),
    workspace_path: Some(PathBuf::from("/workspace/project")),
};

let sources = vec![
    SourceRef {
        source_type: SourceType::DesignDoc,
        location: "docs/prd.md".to_string(),
        priority: 100,
    },
    SourceRef {
        source_type: SourceType::SpecFile,
        location: "docs/api-spec.yaml".to_string(),
        priority: 50,
    },
];

let result = pipeline.inject(&mut ctx, &sources)?;
println!("注入内容长度: {}", result.content.len());
```

## 验收标准

- [ ] `Injector` trait 定义清晰，支持扩展
- [ ] 实现 `FileInjector` 支持 Markdown/JSON/YAML/纯文本
- [ ] 实现 `ProjectContextInjector` 自动检测项目结构
- [ ] 实现 `CompositeInjector` 支持多源合并
- [ ] 内容格式化统一转换为 Markdown
- [ ] 提供与 Task 集成的便捷接口
- [ ] 编写单元测试覆盖核心逻辑

## 依赖与风险

### 依赖
- 需要 `agentdash-domain` 中的 Task/Story 实体
- 需要 `agentdash-executor` 的 ExecutionContext 集成

### 风险
- R1: 注入内容过多导致 Token 超限
  - 缓解：提供内容长度统计，未来增加智能截断
- R2: 文件读取性能问题（大文件）
  - 缓解：设置文件大小限制（如 1MB），大文件跳过或截断

## 后续扩展

1. **智能内容筛选**：基于 Task 描述智能选择相关上下文
2. **缓存机制**：缓存已解析的注入内容
3. **更多注入源**：Git 历史、代码索引、相似任务推荐
4. **模板支持**：Handlebars/Tera 等模板引擎集成

## 参考文档

- `docs/modules/06-injection.md` - 模块原始设计
- `crates/agentdash-domain/src/task/` - Task 实体定义
