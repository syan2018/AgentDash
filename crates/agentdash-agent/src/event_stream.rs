use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

/// 事件流 — 参照 Pi 的 `EventStream<T, R>`
///
/// 基于 `mpsc::unbounded_channel`，提供：
/// - 发送端 (`EventSender`)：在 agent loop 内部推送事件
/// - 接收端 (`EventReceiver`)：外部消费者以 `Stream` 形式读取
pub struct EventSender<T> {
    tx: mpsc::UnboundedSender<T>,
}

impl<T> EventSender<T> {
    pub fn send(&self, event: T) {
        let _ = self.tx.send(event);
    }
}

impl<T> Clone for EventSender<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

pub type EventReceiver<T> = UnboundedReceiverStream<T>;

/// 创建事件流的发送端和接收端
pub fn event_channel<T>() -> (EventSender<T>, EventReceiver<T>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (EventSender { tx }, UnboundedReceiverStream::new(rx))
}
