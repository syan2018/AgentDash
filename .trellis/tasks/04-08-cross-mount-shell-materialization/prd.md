# 跨 Mount Shell 访问：云端内容本地化

> 状态：planning
> 前置依赖：`04-08-skill-discovery-injection`、`04-08-tool-execution-improvements`

## 问题背景

### 架构现状

`shell_exec` 是挂载作用域的：命令实际运行在某个 mount 对应的后端上（通过 relay 协议），只能访问该 mount 所在后端的文件系统。

```
Mount A（本机/远程工作区）
  provider: relay_fs
  root_ref: /workspace/repo
  capabilities: [Read, Write, List, Search, Exec]   ← shell 在这里跑

Mount B（云端技能库）
  provider: relay_fs / external_service
  root_ref: s3://bucket/skills
  capabilities: [Read, List]                         ← 无 Exec，shell 看不到这里
```

当一个存放在 Mount B 的 skill 写了 `cat ./templates/context.md`，bash 经由 Mount A 执行，根本看不到 Mount B 的内容。

### 具体触发场景

1. Skill 寄存在云端工作空间（如 s3://bucket/skills/code-review/）
2. SKILL.md 里引用了相对路径的辅助文件（./templates/context.md、./scripts/check.sh）
3. 模型生成 bash 命令去读这些文件（`cat ./templates/context.md`）
4. 命令在本机/工作区 mount 上执行 → 文件不存在 → 失败

---

## 为什么通用路径重写方案有问题

在实现前需明确：**"将 bash 命令里的所有文件路径替换为本地物化路径"是无法可靠实现的**。

Bash 命令中文件引用的形式无法穷举：
```bash
cat ./context.md                    # 简单相对路径
cat $(ls templates/)                # 子 shell 展开
TPATH=./doc.md; cat $TPATH          # 变量引用  
awk -f ./processor.awk input.txt    # flag 参数
source ./init.sh                    # source 指令
```

可靠解析 arbitrary bash 提取所有文件引用，实质上是 bash 静态分析，属于 open problem。**不做此方向。**

---

## 分层设计

### 层 1（推荐实现）：Skill 加载时预取 + Staging

**在 skill loader 层解决**，而非在 bash 执行层。

当 skill loader 发现 SKILL.md 时，扫描其依赖声明，通过 `read_file` 工具预取到 session-scoped staging 目录：

**SKILL.md frontmatter 扩展**：

```yaml
---
name: code-review
description: "代码审查 skill"
assets:
  - templates/review-template.md
  - templates/checklist.md
  # 支持 glob: templates/**/*.md
---
```

**Skill Staging 流程**：

```
1. skill loader 读取 SKILL.md（通过 read_file 工具，走 relay_service）
2. 解析 assets 字段的相对路径列表
3. 对每个 asset：
   a. 通过 relay_service.read_text(mount_id, asset_relative_path) 读取内容
   b. 写入 session staging 目录：
      /tmp/agentdash-{session_id}/mounts/{mount_id}/{relative_path}
4. 生成路径映射表注入到 skill 内容里：
   <!-- staged assets:
     ./templates/review-template.md → /tmp/agentdash-abc/mounts/skills/templates/review-template.md
   -->
```

**Staging 目录生命周期**：session 结束时清理（SessionTerminal hook）。

**约束**：只对 `Read`-capable mount 有效；`List` 能力配合 glob assets 使用。

---

### 层 2（配合 ShellExecInterceptor）：显式 URI 路径重写

对命令中出现**显式 mount URI 格式**（`mount_id://path`）的情况做物化 + 替换：

```rust
// ShellExecInterceptor 中
// 检测 pattern: (?<mount_id>\w+)://(?<path>[^\s"']+)
// 匹配到 → 物化文件 → 替换为 staging 本地路径

// 能处理:
// cat skills://code-review/templates/context.md
//   → cat /tmp/agentdash-abc/mounts/skills/code-review/templates/context.md

// 不处理:
// cat ./context.md   （需要知道 cwd 映射关系，超出范围）
```

**系统 prompt 约束**（配合使用）：
```
当需要读取非执行 mount 上的文件时，必须使用显式 mount URI 格式：
  read_file skills://code-review/templates/context.md
  或在 bash 中：cat skills://code-review/templates/context.md
不要使用相对路径访问跨 mount 内容。
```

---

### 层 3（近期缓解）：系统 Prompt 指导

在 build_runtime_system_prompt() 里对有云端 mount 的 session 追加规则：

```
## 跨 Mount 文件访问规范

当前 session 包含以下 mount：
- main（可执行）: 工作区根目录
- skills（只读）: 技能库，路径格式 skills://...

访问非执行 mount 的文件时：
- 优先使用 read_file 工具（如 read_file skills://foo/bar.md）
- 在 bash 中使用显式 URI：cat skills://foo/bar.md
- 不要使用相对路径访问跨 mount 内容
```

此层不需要任何底层改动，能覆盖 80% 的场景。

---

## 实施顺序

```
1. 层 3（系统 prompt 指导）—— 立即可做，无依赖
2. 层 1（skill assets 预取）—— 依赖 04-08-skill-discovery-injection 的 skill loader
3. 层 2（ShellExecInterceptor URI 重写）—— 依赖 04-08-tool-execution-improvements 的 ShellExecInterceptor
```

---

## 刻意不做的部分

| 排除项 | 原因 |
|--------|------|
| 通用相对路径 bash 重写 | 需要 bash 静态分析，不可靠 |
| FUSE 挂载 | 需要系统权限，运维复杂度高 |
| 写回云端 | 用户要求只读，写回会引入一致性问题 |
| 跨 mount 的 cwd 上下文追踪 | 超出当前 agent loop 的架构范围 |

---

## 潜在风险

1. **Staging 目录空间**：如果 skill assets 很多或很大，staging 占用本地磁盘。需要设置单次预取的大小上限（如 10MB per skill，可配置）。

2. **物化内容过期**：Session 运行过程中云端文件被修改，staging 是快照。对于 skill 内容这一般可接受（skill 是稳定的配置），但需在文档中说明。

3. **mount_id 与命令歧义**：`skills://foo` 中的 `skills` 是 mount_id 还是 URL scheme？需要在路径解析时明确区分（mount_id 不应与 http/https/ftp 等标准 scheme 冲突，可用 allow-list 或强制 mount_id 不含 `://`）。

4. **ShellExecInterceptor 与 BeforeTool hook 的关系**：两者都会修改 shell 命令，执行顺序需要明确定义（建议 interceptor 在 BeforeTool hook 之后执行，作为最终的 materialization pass）。
