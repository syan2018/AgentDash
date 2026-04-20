# Address Space 内核能力拓展：Watch / Projection / Link / Metadata

## 背景

当前 Address Space 体系已经实现了 Mount 组合、Provider SPI、Capability 访问控制、URI 寻址等核心文件系统抽象。但对比真实文件系统的能力模型，仍有四项高价值能力缺失，限制了 Agent Workflow 场景下的数据流转效率和系统自省能力。

本任务将这四项能力作为 Address Space 内核层的拓展，在 SPI trait、Domain 类型、Application 层做最小必要的扩展。

---

## 能力 1：Watch / 事件通知

### 动机

当前 Address Space 是纯被动存储——文件变更后，没有任何机制通知关注方。在 Workflow 场景下，Step A 写了输出后 Step B 需要外部编排显式触发才能感知。

类比：inotify/FSEvents 让 `webpack --watch` 成为可能；没有它就只能轮询。

### 目标

Provider 可选地支持文件变更事件推送，上层消费者通过统一接口订阅。

### 设计方向

**SPI 层扩展**：

```rust
/// 文件变更事件。
#[derive(Debug, Clone)]
pub struct MountEvent {
    pub mount_id: String,
    pub path: String,
    pub kind: MountEventKind,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy)]
pub enum MountEventKind {
    Created,
    Modified,
    Deleted,
    Renamed,   // 可选，不是所有 provider 都能区分
}

// MountProvider trait 新增可选方法
async fn watch(
    &self,
    mount: &Mount,
    path: &str,   // 空串 = mount 根目录
    ctx: &MountOperationContext,
) -> Result<tokio::sync::mpsc::Receiver<MountEvent>, MountError> {
    Err(MountError::NotSupported(...))
}
```

**MountCapability 新增**：

```rust
pub enum MountCapability {
    Read, Write, List, Search, Exec,
    Watch,  // 新增
}
```

**各 Provider 实现评估**：

| Provider | Watch 支持 | 实现方式 |
|----------|-----------|---------|
| `relay_fs` | 可支持 | Relay backend 转发本地 fs watch 事件 |
| `inline_fs` | 自动支持 | InlineContentOverlay write/delete 时发 event |
| `lifecycle_vfs` | 可支持 | LifecycleRun 状态变更时发 event |
| `canvas_fs` | 可支持 | Canvas 写入时发 event |
| 外部插件 | 按需 | 插件自行实现 |

**消费侧**：暂不做 Agent tool 暴露（Agent 不需要主动 watch），主要供 Application 层内部的 Workflow 编排引擎消费。

### 验收标准

- [ ] `MountProvider` trait 有 `watch` 可选方法
- [ ] `MountCapability` 枚举包含 `Watch`
- [ ] `InlineContentOverlay` 在 write/delete 时发出 `MountEvent`
- [ ] 至少一个消费侧 demo（如 lifecycle 状态变更触发日志）

---

## 能力 2：Projection 泛化（/proc 风格计算文件）

### 动机

`lifecycle_vfs` 已经实现了"运行时状态伪装成文件"的模式，但它是一个特化实现。没有泛化的"计算文件"概念，每种系统信息都需要单独造一个 API 或 tool。

类比：Linux `/proc/cpuinfo` 不是磁盘上的文件，是内核状态的实时投影。读它和读普通文件用同一个 `read()` 系统调用。

### 目标

让 Provider 能声明某些路径为"读取时计算"的虚拟文件，Agent 用同一套 `fs.read` 工具既能读文件也能读系统状态。

### 设计方向

这不需要新的 SPI 方法——`read_text` 本身就可以返回计算内容。核心是**约定 + 文档 + 示例 Provider**。

**约定**：

- Provider 的 `read_text` 可以返回动态内容（不要求内容不变）
- `list` 可以返回不存在于物理存储的虚拟条目
- `RuntimeFileEntry` 对于 projection 文件，`size` 和 `modified_at` 可以为 `None`（内容是动态的，size 无意义）

**示例拓展方向**（不在本任务实现，但 PRD 描述场景）：

- `lifecycle_vfs` 已有：`runs/{id}/status`, `nodes/{key}/state`
- 未来可加：`_meta/mounts` 返回当前 address space 的 mount 列表（自省）
- 未来可加：`_meta/capabilities` 返回当前 mount 的能力声明
- 插件 provider 可以把 API 响应投影为文件

**RuntimeFileEntry 调整**：

```rust
pub struct RuntimeFileEntry {
    pub path: String,
    pub size: Option<u64>,
    pub modified_at: Option<i64>,
    pub is_dir: bool,
    pub is_virtual: bool,  // 新增：标记此条目为 projection/computed
}
```

### 验收标准

- [ ] `RuntimeFileEntry` 增加 `is_virtual` 字段
- [ ] spec 文档中明确 projection 约定（Provider 的 read_text 可返回动态内容）
- [ ] `lifecycle_vfs` 中已有的 projection 行为标记为 `is_virtual = true`

---

## 能力 3：引用链接（Mount 级 Symbolic Link）

### 动机

Workflow 中 Step B 需要读取 Step A 的输出，当前靠 context binding 机制在 session 构建时复制或绑定数据。如果 address space 层面支持"路径 A 指向 mount B 的路径 C"的引用，可以实现零拷贝的跨 mount 数据引用。

### 目标

在 Address Space 构建阶段支持声明式的 mount 级引用，运行时透明解析。

### 设计方向

**不做通用 symlink**（避免循环检测、权限穿透等复杂性），而是做 **Mount Alias**——在 Vfs 构建时声明"mount X 的 path Y 实际读取 mount Z 的 path W"。

**Domain 层**：

```rust
/// 挂载级别的路径引用（声明式，在 address space 构建时定义）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountLink {
    /// 来源 mount + path（用户看到的路径）
    pub from_mount_id: String,
    pub from_path: String,
    /// 目标 mount + path（实际读取的位置）
    pub to_mount_id: String,
    pub to_path: String,
    /// 是否跟随目标更新（true = 每次读取都从目标实时取；false = 构建时快照）
    pub follow: bool,
}

pub struct Vfs {
    pub mounts: Vec<Mount>,
    pub default_mount_id: Option<String>,
    pub links: Vec<MountLink>,  // 新增
    // ...
}
```

**解析层**（path.rs）：

- `resolve_mount_uri` 在返回 `ResourceRef` 前检查 links 表
- 命中 link 则透明重定向到目标 mount+path
- 循环检测：限制最大跳转深度（如 5 层），超过即报错

**典型场景**：

- Workflow step input 声明为 link → 上游 step output mount 的某路径
- Agent knowledge 引用 project 级别的共享文档（不复制）
- Canvas 引用 workspace 文件（只读视图）

### 验收标准

- [ ] `Vfs` 包含 `links: Vec<MountLink>` 字段
- [ ] `resolve_mount_uri` 支持 link 解析，有深度限制
- [ ] address space 构建函数能注入 link 声明
- [ ] 单元测试覆盖：正常跳转、循环检测、目标 mount 不存在

---

## 能力 5：Extended Attributes（文件级元数据）

### 动机

当前 `RuntimeFileEntry` 只有 path/size/modified_at/is_dir，单个文件没有自定义属性。实际场景中，外部 Provider（如 KM）有丰富的文件元数据（作者、标签、来源 URL 等），目前只能以 YAML frontmatter 嵌入文件内容开头，导致内容污染、搜索干扰、写回困难。

类比：POSIX xattr 让元数据和内容走不同通道——`getfattr` 读元数据，`cat` 读内容，互不干扰。

### 目标

文件元数据与文件内容分离，通过独立通道读写。

### 设计方向

**SPI 层**：

```rust
// RuntimeFileEntry 扩展
pub struct RuntimeFileEntry {
    pub path: String,
    pub size: Option<u64>,
    pub modified_at: Option<i64>,
    pub is_dir: bool,
    pub is_virtual: bool,
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,  // 新增
}

// ReadResult 扩展
pub struct ReadResult {
    pub path: String,
    pub content: String,
    pub attributes: Option<serde_json::Map<String, serde_json::Value>>,  // 新增
}

// MountProvider trait 新增可选方法
async fn stat(
    &self,
    mount: &Mount,
    path: &str,
    ctx: &MountOperationContext,
) -> Result<RuntimeFileEntry, MountError> {
    Err(MountError::NotSupported(...))
}
```

**约定**：

- `read_text` 返回时可附带 attributes（如果 Provider 支持）
- `stat` 只返回元数据，不读内容（轻量查询）
- `list` 返回的 entries 可以带 attributes（Provider 按成本决定是否填充）
- Agent tool `fs.read` 的输出中，metadata 和 content 分区展示

**各 Provider 的 attributes 示例**：

| Provider | 典型 attributes |
|----------|----------------|
| `relay_fs` | `git_author`, `git_last_modified`, `encoding` |
| `inline_fs` | `created_by`, `container_id` |
| `lifecycle_vfs` | `run_id`, `node_key`, `port_key`, `status` |
| KM 插件 | `km_id`, `author`, `tags`, `url`, `department` |

**MountCapability**：不需要新增——stat/attributes 是 Read 能力的细化，不是新能力。

### 验收标准

- [ ] `RuntimeFileEntry` 增加 `attributes` 可选字段
- [ ] `ReadResult` 增加 `attributes` 可选字段
- [ ] `MountProvider` 增加 `stat` 可选方法（默认 NotSupported）
- [ ] KM 插件 provider 不再需要 frontmatter 嵌入，改用 attributes 返回元数据
- [ ] Agent tool `fs.read` 输出中 metadata 与 content 分离展示

---

## 实现优先级

| 能力 | 优先级 | 原因 |
|------|--------|------|
| 5. Extended Attributes | P0 | 解决当前 KM frontmatter 污染的实际痛点，改动最小 |
| 2. Projection 泛化 | P0 | 只需加 `is_virtual` 字段 + 文档约定，几乎零成本 |
| 3. Mount Link | P1 | 需要 path 解析层改动，但逻辑清晰 |
| 1. Watch 事件 | P1 | 需要 async channel 基础设施，但 trait 扩展本身简单 |

## 不在范围内

- Agent tool 层的 watch 订阅命令（Agent 不需要主动 watch，编排层消费即可）
- 多层 OverlayFS 式叠加
- Content addressing / 去重
- Streaming I/O
