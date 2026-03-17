use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::sync::broadcast;
use tokio_stream::Stream;
use tokio_stream::wrappers::BroadcastStream;

const DEFAULT_CHANNEL_CAPACITY: usize = 4096;

/// 事件流 — 对齐 Pi 的多订阅者事件分发模型
///
/// 基于 `broadcast::channel`，支持：
/// - 发送端 (`EventSender`)：在 agent loop 内部推送事件
/// - 接收端 (`EventReceiver`)：外部消费者以 `Stream` 形式读取
/// - 多订阅者：通过 `EventSender::subscribe()` 创建额外接收端
pub struct EventSender<T: Clone + Send + 'static> {
    tx: broadcast::Sender<T>,
}

impl<T: Clone + Send + 'static> EventSender<T> {
    pub fn send(&self, event: T) {
        let _ = self.tx.send(event);
    }

    /// 创建新的事件接收端 — 对齐 Pi `Agent.subscribe(fn)`
    pub fn subscribe(&self) -> EventReceiver<T> {
        EventReceiver {
            inner: BroadcastStream::new(self.tx.subscribe()),
        }
    }
}

impl<T: Clone + Send + 'static> Clone for EventSender<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

/// 事件接收端 — 实现 `Stream<Item = T>` 供下游透明消费
///
/// 内部自动跳过 lagged 错误（消费者过慢时丢弃的旧事件）。
pub struct EventReceiver<T: Clone + Send + 'static> {
    inner: BroadcastStream<T>,
}

impl<T: Clone + Send + 'static> Stream for EventReceiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        loop {
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(item))) => return Poll::Ready(Some(item)),
                Poll::Ready(Some(Err(_))) => continue,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// 创建事件流的发送端和一个初始接收端
pub fn event_channel<T: Clone + Send + 'static>() -> (EventSender<T>, EventReceiver<T>) {
    let (tx, rx) = broadcast::channel(DEFAULT_CHANNEL_CAPACITY);
    (
        EventSender { tx },
        EventReceiver {
            inner: BroadcastStream::new(rx),
        },
    )
}
