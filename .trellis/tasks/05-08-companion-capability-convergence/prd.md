# Companion 能力收束 + 管道标准化 PRD

## 已完成工作

### Phase 1-3: Companion 收束（本次实现）

- CapabilityResolverInput 新增 `available_companions` + `allowed_companions_filter`
- CapabilityResolverOutput 新增 `companion_agents`
- Resolver 内部按 `CAP_COLLABORATION` + filter 过滤产出
- Assembler `compose_owner_bootstrap` 预查询 agent_links → 注入 resolver
- 删除 HookProvider Markdown 注入 + baseline_capabilities 反向解析
- 改用 context_bundle fragment 输出
- 统一 CompanionSliceMode 定义位置（canonical in resolver.rs）
- Companion dispatch 改用 `compose_companion_dispatch` + 标准 `launch_prompt_with_intent`
- companion_agents 归入 SessionProfile（session 级静态配置）
- 前端无破坏性变更

## 当前数据结构审查

### 一个"资源维度"的完整层级

```
ResolverInput    →  输入候选 + 过滤条件
ResolverOutput   →  解析产物
Builder          →  累积器（build pattern 暂态）
PreparedInputs   →  组合产物（builder.build()）
PromptRequest    →  pipeline 入参（merge base + prepared）
SessionProfile   →  session 级缓存
```

### 当前已存在的资源维度

| 维度 | Input 字段 | Output 字段 | 备注 |
|------|-----------|-------------|------|
| MCP Presets | `available_presets` | `custom_mcp_servers` | 预查询 + key 解析 |
| Companion | `available_companions` + filter | `companion_agents` | 预查询 + visibility 过滤 |
| Platform MCP | （隐含在 effective_caps） | `platform_mcp_configs` | 从 cap key 映射 |
| Agent MCP | `agent_mcp_servers` | （合入 custom_mcp_servers） | 回退数据源 |
| Workflow | `workflow_ctx` | （影响 effective_caps） | directive 归约 |
| Companion Slice | `companion_slice_mode` | （裁剪 flow_capabilities） | 子 session 裁剪 |

### 问题：每新增一个维度的代价

1. CapabilityResolverInput 加 1-2 字段
2. CapabilityResolverOutput 加 1 字段
3. SessionAssemblyBuilder 加 1 字段 + with_xxx 方法
4. PreparedSessionInputs 加 1 字段
5. PromptSessionRequest 加 1 字段
6. SessionProfile 加 1 字段（如果需要跨 turn 缓存）
7. finalize_request 加 1 merge 逻辑

共计 5-7 处散点改动。

---

## 后续方案：Resolver Extension Trait

### 设计目标

新维度通过实现 trait + 注册来扩展，而非修改 5-7 个 struct。

### 核心抽象

```rust
/// 能力解析的资源维度扩展点。
///
/// 每个维度实现此 trait，描述：
/// - 输入数据类型（assembler 预查询产出）
/// - 输出数据类型（resolver 解析后产出）
/// - 解析逻辑（effective_caps → output）
pub trait ResourceDimension: Send + Sync + 'static {
    /// 维度标识键（用于 BTreeMap 索引）。
    fn key(&self) -> &'static str;

    /// 从 resolver 有效能力集和输入数据中解析出本维度的产出。
    ///
    /// input_data: 由 assembler 预查询写入的 type-erased 数据
    /// effective_caps: 当前 session 的有效能力集合
    fn resolve(
        &self,
        effective_caps: &BTreeSet<ToolCapability>,
        input_data: &dyn std::any::Any,
    ) -> Box<dyn std::any::Any + Send>;
}
```

### 结构变化

```rust
pub struct CapabilityResolverInput {
    // 核心字段保留（owner_ctx, agent_declared_capabilities, workflow_ctx, ...）
    pub owner_ctx: SessionOwnerCtx,
    pub agent_declared_capabilities: Option<Vec<String>>,
    pub workflow_ctx: SessionWorkflowContext,
    pub companion_slice_mode: Option<CompanionSliceMode>,

    // 资源维度输入统一为 typed bag
    pub dimension_inputs: ResourceInputBag,
}

pub struct CapabilityResolverOutput {
    // 核心输出保留
    pub flow_capabilities: FlowCapabilities,
    pub platform_mcp_configs: Vec<McpInjectionConfig>,
    pub effective_capabilities: BTreeSet<ToolCapability>,

    // 资源维度输出统一为 typed bag
    pub dimension_outputs: ResourceOutputBag,
}

/// Type-safe 资源袋（内部 BTreeMap<&'static str, Box<dyn Any>>）。
pub struct ResourceInputBag { /* ... */ }
pub struct ResourceOutputBag { /* ... */ }

impl ResourceOutputBag {
    /// 类型安全 accessor。
    pub fn get<T: 'static>(&self, key: &str) -> Option<&T> { /* downcast */ }
}
```

### 维度注册示例

```rust
pub struct CompanionDimension;

impl ResourceDimension for CompanionDimension {
    fn key(&self) -> &'static str { "companion" }

    fn resolve(
        &self,
        effective_caps: &BTreeSet<ToolCapability>,
        input_data: &dyn Any,
    ) -> Box<dyn Any + Send> {
        let input = input_data.downcast_ref::<CompanionDimensionInput>()
            .expect("CompanionDimension requires CompanionDimensionInput");
        let result = resolve_companion_agents(effective_caps, &input.available, &input.filter);
        Box::new(result)
    }
}

pub struct McpPresetDimension;

impl ResourceDimension for McpPresetDimension {
    fn key(&self) -> &'static str { "mcp_preset" }

    fn resolve(
        &self,
        effective_caps: &BTreeSet<ToolCapability>,
        input_data: &dyn Any,
    ) -> Box<dyn Any + Send> {
        let presets = input_data.downcast_ref::<AvailableMcpPresets>()
            .expect("McpPresetDimension requires AvailableMcpPresets");
        // ... MCP key 解析逻辑
        Box::new(resolved_servers)
    }
}
```

### Resolver 内部执行

```rust
impl CapabilityResolver {
    pub fn resolve(
        input: &CapabilityResolverInput,
        platform: &PlatformConfig,
        dimensions: &[&dyn ResourceDimension],  // 注册的维度列表
    ) -> CapabilityResolverOutput {
        // 1. 核心逻辑不变：baseline → directive reduction → effective_caps
        let effective_caps = ...;
        let flow_capabilities = ...;

        // 2. 遍历所有注册维度执行 resolve
        let mut dimension_outputs = ResourceOutputBag::new();
        for dim in dimensions {
            if let Some(input_data) = input.dimension_inputs.get_raw(dim.key()) {
                let output = dim.resolve(&effective_caps, input_data);
                dimension_outputs.insert(dim.key(), output);
            }
        }

        CapabilityResolverOutput {
            flow_capabilities,
            platform_mcp_configs,
            effective_capabilities: effective_caps,
            dimension_outputs,
        }
    }
}
```

### 下游消费（Builder / PreparedInputs / SessionProfile）

```rust
// SessionAssemblyBuilder 只需一个泛型方法
impl SessionAssemblyBuilder {
    pub fn with_dimension_output<T: Clone + Send + 'static>(
        &mut self, key: &'static str, value: T
    ) -> &mut Self { /* ... */ }
}

// PreparedSessionInputs / PromptSessionRequest 同理
pub struct PreparedSessionInputs {
    // ... 核心字段
    pub dimension_bag: ResourceOutputBag,
}
```

### 权衡分析

| 维度 | 当前模式 | Extension Trait |
|------|---------|-----------------|
| 新增维度代码量 | 5-7 处 struct 改动 | 1 个 trait impl + 1 处注册 |
| 编译时类型安全 | ✅ 完整 | ⚠️ downcast 有运行时 panic 风险 |
| 可调试性 | ✅ 字段直接可见 | ⚠️ type-erased 需 Debug impl |
| 学习曲线 | 低（直白字段） | 中（需理解 dimension 抽象） |
| 适用规模 | <10 维度合理 | >10 维度时优势明显 |

### 建议实施路线

1. **短期（当前）**：保持已有的 companion/mcp_preset 显式字段，它们已稳定
2. **中期触发条件**：当第 4 个资源维度出现（如 skills/knowledge/tool_presets）时启动迁移
3. **迁移策略**：先 impl trait 对现有维度做 adapter，新维度只走 trait 路径；
   在所有显式字段迁移完后删除旧字段

---

## 可能纳入的散落 feature

| Feature | 当前位置 | 收束后 |
|---------|---------|--------|
| **Skills discovery** | prompt_pipeline 中 inline 扫描 | ResourceDimension: SkillsDimension |
| **Knowledge sources** | 散落在 context_builder | ResourceDimension: KnowledgeDimension |
| **Agent MCP servers** | agent_mcp_servers 字段 | 合并入 McpPresetDimension |
| **Platform MCP** | 隐含映射 | 独立 PlatformMcpDimension |

## 状态

- [x] Phase 1-3 实现完毕
- [x] companion_agents 归入 SessionProfile
- [ ] Extension Trait 设计待后续 task 启动实施
