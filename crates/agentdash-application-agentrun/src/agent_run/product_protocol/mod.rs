//! AgentRun 产品协议与 durable orchestration。
//!
//! 本模块是 Product owner 面向 Runtime Contract 投影、AgentRun fork 与 Companion dispatch
//! 的唯一生产入口。具体 Runtime、Host、Business Surface 与持久化 adapter 由 S5
//! composition root 注入，产品层不选择 legacy runtime 路径。

mod activation;
mod companion;
mod feed;
mod fork_saga;
mod production_adapters;

pub use activation::*;
pub use companion::*;
pub use feed::*;
pub use fork_saga::*;
pub use production_adapters::*;
