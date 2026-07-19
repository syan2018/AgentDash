use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

/// Routine 触发器提供者 — 插件通过实现此 trait 注册自定义事件源。
///
/// 生命周期：
/// 1. 插件注册时，宿主调用 `provider_key()` 获取唯一标识
/// 2. 用户创建 Routine 时选择 provider_key，宿主调用 `validate_config()` 校验配置
/// 3. 宿主调用 `start_listening()` 启动事件监听
/// 4. 事件到达时，provider 调用 `fire_callback` 通知宿主触发 Routine
/// 5. 服务关闭时，宿主调用 `stop_listening()`
#[async_trait]
pub trait RoutineTriggerProvider: Send + Sync {
    /// 唯一标识，格式 `plugin_name:trigger_type`
    fn provider_key(&self) -> &str;

    /// 人类可读的显示名称
    fn display_name(&self) -> &str;

    /// 该 provider 支持的配置 schema（JSON Schema 子集），供前端渲染配置表单
    fn config_schema(&self) -> serde_json::Value;

    /// 校验用户提交的 provider_config 是否合法
    fn validate_config(&self, config: &serde_json::Value) -> Result<(), String>;

    /// 启动对指定 Routine 的事件监听。
    /// `fire_callback` 是宿主提供的回调，provider 在事件到达时调用它。
    async fn start_listening(
        &self,
        routine_id: Uuid,
        config: &serde_json::Value,
        fire_callback: Arc<dyn RoutineFireCallback>,
    ) -> Result<(), String>;

    /// 停止对指定 Routine 的事件监听
    async fn stop_listening(&self, routine_id: Uuid) -> Result<(), String>;
}

/// 宿主提供给 provider 的回调接口
#[async_trait]
pub trait RoutineFireCallback: Send + Sync {
    /// 触发 Routine 执行。
    /// `trigger_source`: 事件标识（如 `"github:pull_request.opened"`）
    /// `payload`: 事件数据，将用于 prompt 模板插值和 entity_key 提取
    /// 返回 RoutineExecution ID
    async fn fire(
        &self,
        trigger_source: String,
        payload: serde_json::Value,
    ) -> Result<Uuid, String>;
}
