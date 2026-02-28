# @引用工作空间文件

## 背景与目标

作为上下文注入机制的早期替代方案，我们需要在 Agent 会话框中支持通过 `@` 语法引用工作空间中的文件，将文件内容作为上下文传递给 Agent。

使用场景：
- 用户在 Session 页面输入提示词时，想引用某个代码文件
- 用户在 Task 执行前，想附加特定的上下文文件
- 快速查看文件内容而无需离开当前会话界面

## 目标

1. 在 Prompt 输入框支持 `@` 触发文件选择
2. 读取工作空间文件并注入到 Session 上下文
3. 视觉反馈：显示已引用的文件列表
4. 支持多文件引用和删除

## 非目标

- 不实现智能文件推荐（基于内容相关性）
- 不实现文件内容预览弹窗
- 不做文件夹递归引用
- 不实现跨工作空间文件引用

## 当前状态分析

### 已有基础
- ✅ Session 页面有 Prompt 输入框（textarea）
- ✅ `promptSession` API 支持发送 Prompt
- ✅ 工作空间管理已有 API（`GET /workspaces/{id}/files` 等）

### 需要补齐
- ❌ 前端 `@` 触发文件选择 UI
- ❌ 后端读取文件并注入到 Prompt
- ❌ 文件引用可视化

## 需求规格

### FR-1: @触发文件选择

在 Prompt 输入框输入 `@` 时触发文件选择浮层：

```
用户输入: "请帮我分析 @"
             │
             ▼
        ┌─────────────────────────────────────┐
        │ 📁 选择文件                          │
        │ ─────────────────────────────────   │
        │ 📄 src/main.rs                       │
        │ 📄 src/lib.rs                        │
        │ 📁 src/components/                   │
        │ 📄 README.md                         │
        │ 📄 Cargo.toml                        │
        │                                     │
        │ [搜索: ______]                      │
        └─────────────────────────────────────┘
```

交互细节：
- `@` 后输入字符时实时过滤文件列表
- 方向键上下选择，Enter 确认
- ESC 或点击外部关闭
- 支持路径自动补全（如 `@src/comp` → 显示 `src/components/` 下的文件）

### FR-2: 引用格式与解析

选定文件后，在输入框中插入占位符：

```
请帮我分析 @src/main.rs 中的错误处理逻辑
```

占位符格式：`@path/to/file`（不含空格）

发送 Prompt 前，前端将占位符展开为结构化数据：

```typescript
interface PromptWithReferences {
  prompt: string;  // 原始文本（不含占位符或占位符替换为文件名）
  references: FileReference[];
}

interface FileReference {
  path: string;           // 文件路径（相对工作空间根目录）
  content?: string;       // 文件内容（可选，由后端读取）
  workspaceId: string;    // 所属工作空间
}
```

### FR-3: API 扩展

扩展 `PromptSessionRequest` 支持文件引用：

```rust
// POST /sessions/{session_id}/prompt
pub struct PromptSessionRequest {
    pub prompt: String,
    pub working_dir: Option<String>,
    pub env: HashMap<String, String>,
    pub executor_config: Option<ExecutorConfig>,
    /// 新增：引用的文件列表
    pub file_references: Option<Vec<FileReference>>,
}

pub struct FileReference {
    pub path: String,
    pub workspace_id: String,
}
```

后端处理：
1. 接收请求后，读取每个引用的文件内容
2. 将文件内容格式化为注入上下文
3. 将格式化的上下文附加到 Prompt 前发送给 Agent

文件内容格式化示例：

```markdown
---
文件: src/main.rs
---
```rust
fn main() {
    println!("Hello, world!");
}
```

---

原始 Prompt:
请帮我分析其中的错误处理逻辑
```

### FR-4: 可视化反馈

Prompt 输入框下方显示已引用的文件列表：

```
┌──────────────────────────────────────────────────────────────┐
│ 请帮我分析 @src/main.rs 中的错误处理逻辑                        │
└──────────────────────────────────────────────────────────────┘

📎 已引用文件 (1)
┌──────────────────────────────────────────────────────────────┐
│ 📄 src/main.rs                                    [×]        │
└──────────────────────────────────────────────────────────────┘
```

点击 [×] 删除引用，同时从 Prompt 文本中移除对应的 `@path`。

### FR-5: 工作空间上下文

文件选择器默认使用当前 Task 绑定的 Workspace：

```typescript
// 获取文件列表的 API
GET /workspaces/{workspace_id}/files?pattern=*

// 返回
interface WorkspaceFilesResponse {
  files: WorkspaceFile[];
}

interface WorkspaceFile {
  path: string;      // 相对路径
  name: string;      // 文件名
  type: 'file' | 'directory';
  size?: number;     // 文件大小（字节）
}
```

如果没有绑定 Workspace，显示提示："请先绑定工作空间"。

### FR-6: 文件大小限制

为避免 Token 超限，设置文件大小限制：

| 限制项 | 值 | 说明 |
|--------|-----|------|
| 单文件大小 | 100KB | 超过则拒绝引用并提示 |
| 总引用大小 | 500KB | 所有引用文件总和 |
| 引用文件数 | 10个 | 单次 Prompt 最多引用 |

超出限制时前端提示："文件过大，建议选择更小的文件或分批引用"。

## 技术方案

### 前端实现

```typescript
// features/acp-session/model/useFileReference.ts
export function useFileReference(workspaceId: string | null) {
  const [references, setReferences] = useState<FileReference[]>([]);
  const [isPickerOpen, setIsPickerOpen] = useState(false);

  const addReference = (path: string) => { ... };
  const removeReference = (path: string) => { ... };
  const parsePrompt = (rawPrompt: string) => { ... };

  return { references, isPickerOpen, addReference, removeReference, parsePrompt };
}

// 文件选择器组件
interface FilePickerProps {
  workspaceId: string;
  onSelect: (path: string) => void;
  onClose: () => void;
}

export function FilePicker({ workspaceId, onSelect, onClose }: FilePickerProps) {
  // 使用 useQuery 获取文件列表
  // 支持搜索过滤
  // 键盘导航
}
```

### 后端实现

```rust
// crates/agentdash-executor/src/hub.rs

impl ExecutorHub {
    pub async fn prompt_with_references(
        &self,
        session_id: &str,
        request: PromptSessionRequest,
    ) -> Result<(), ConnectorError> {
        let mut full_prompt = request.prompt.clone();

        // 如果有文件引用，读取并格式化
        if let Some(refs) = &request.file_references {
            let mut context_parts = vec![];

            for file_ref in refs {
                let content = self.read_workspace_file(
                    &file_ref.workspace_id,
                    &file_ref.path
                ).await?;

                // 检查文件大小
                if content.len() > 100_000 {
                    return Err(ConnectorError::FileTooLarge(file_ref.path.clone()));
                }

                let formatted = format_file_context(&file_ref.path, &content);
                context_parts.push(formatted);
            }

            // 合并上下文和原始 Prompt
            full_prompt = format!(
                "{}\n\n---\n\n{}",
                context_parts.join("\n\n---\n\n"),
                request.prompt
            );
        }

        // 发送给 Agent
        self.connector.prompt(session_id, full_prompt).await
    }

    async fn read_workspace_file(
        &self,
        workspace_id: &str,
        path: &str
    ) -> Result<String, ConnectorError> {
        // 通过 WorkspaceManager 读取文件
        // 确保安全：只能读取工作空间内的文件
    }
}

fn format_file_context(path: &str, content: &str) -> String {
    let ext = Path::new(path).extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt");

    format!(
        "---\n文件: {}\n---\n```{}{}\n{}\n```",
        path,
        if ext == "rs" { "rust" } else { ext },
        if ext == "rs" { "" } else { "" },
        content
    )
}
```

### 安全考虑

1. **路径安全**：只允许读取工作空间内的文件，禁止 `../` 等路径遍历
2. **文件类型限制**：禁止读取二进制文件（图片、可执行文件等）
3. **敏感信息过滤**：可选，检测并提示 `.env`、`*secret*` 等文件

```rust
fn validate_file_path(workspace_path: &Path, user_path: &str) -> Result<PathBuf, Error> {
    let canonical_workspace = workspace_path.canonicalize()?;
    let requested = workspace_path.join(user_path).canonicalize()?;

    // 确保请求的文件在工作空间内
    if !requested.starts_with(&canonical_workspace) {
        return Err(Error::AccessDenied);
    }

    Ok(requested)
}
```

## 交互流程

```
用户场景: 在 Session 页面引用文件

┌─────────────┐          ┌─────────────┐          ┌─────────────┐
│   前端页面   │          │   后端 API   │          │  工作空间   │
└──────┬──────┘          └──────┬──────┘          └──────┬──────┘
       │                        │                        │
       │  1. 输入 "@"           │                        │
       │───────────────────────>│                        │
       │                        │                        │
       │  2. 获取文件列表        │                        │
       │───────────────────────>│                        │
       │                        │  3. 读取目录            │
       │                        │───────────────────────>│
       │                        │                        │
       │                        │  4. 返回文件列表         │
       │                        │<───────────────────────│
       │                        │                        │
       │  5. 显示文件选择器       │                        │
       │<───────────────────────│                        │
       │                        │                        │
       │  6. 选择文件            │                        │
       │                        │                        │
       │  7. 点击发送            │                        │
       │───────────────────────>│                        │
       │   (prompt + references)│                        │
       │                        │  8. 读取文件内容         │
       │                        │───────────────────────>│
       │                        │                        │
       │                        │  9. 格式化并发送给 Agent │
       │                        │                        │
```

## 验收标准

- [ ] 输入 `@` 弹出文件选择浮层
- [ ] 文件列表支持搜索过滤
- [ ] 选择文件后插入 `@path/to/file` 占位符
- [ ] 已引用文件在输入框下方可视化展示
- [ ] 可删除已引用的文件
- [ ] 后端正确读取文件并格式化注入
- [ ] 文件大小超限有友好提示
- [ ] 路径遍历攻击被阻止

## 依赖与风险

### 依赖
- 需要工作空间管理 API（获取文件列表）
- 需要上下文注入机制（本功能作为早期替代方案，可独立实现）

### 风险
- R1: 大文件导致内存/Token 问题
  - 缓解：严格的大小限制和错误提示
- R2: 路径遍历安全漏洞
  - 缓解：严格的路径验证，只允许工作空间内文件
- R3: 文件编码问题（非 UTF-8）
  - 缓解：尝试多种编码读取，失败时提示"无法读取文件"

## 后续扩展

1. **智能推荐**：根据 Prompt 内容推荐相关文件
2. **代码片段引用**：支持引用文件的特定行范围（`@file.rs#L10-L20`）
3. **最近使用**：记录最近引用的文件，优先显示
4. **文件夹引用**：支持引用整个文件夹（带摘要）

## 参考

- `docs/modules/06-injection.md` - 上下文注入设计
- `.trellis/tasks/02-28-context-injection-design/prd.md` - 上下文注入机制任务
- `frontend/src/pages/SessionPage.tsx` - Session 页面
