/// 轻量 SSE (Server-Sent Events) 行解析器
///
/// 用于解析 reqwest streaming response 中的 SSE 事件流。
/// 支持 OpenAI / Anthropic 的标准 SSE 格式。

#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// 增量式 SSE 解析器 — 可安全地跨 chunk 边界工作
pub struct SseParser {
    buffer: String,
    current_event: Option<String>,
    data_lines: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            current_event: None,
            data_lines: Vec::new(),
        }
    }

    /// 喂入新到达的字节块，返回本次解析出的完整 SSE 事件
    pub fn feed(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();

        loop {
            let Some(line_end) = self.find_line_end() else {
                break;
            };
            let line = self.buffer[..line_end]
                .trim_end_matches('\r')
                .to_string();
            let skip = if self.buffer.as_bytes().get(line_end) == Some(&b'\r')
                && self.buffer.as_bytes().get(line_end + 1) == Some(&b'\n')
            {
                line_end + 2
            } else {
                line_end + 1
            };
            self.buffer = self.buffer[skip..].to_string();

            if line.is_empty() {
                if !self.data_lines.is_empty() || self.current_event.is_some() {
                    events.push(SseEvent {
                        event: self.current_event.take(),
                        data: self.data_lines.join("\n"),
                    });
                    self.data_lines.clear();
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            if let Some(val) = line.strip_prefix("event:") {
                self.current_event = Some(strip_leading_space(val).to_string());
            } else if let Some(val) = line.strip_prefix("data:") {
                self.data_lines
                    .push(strip_leading_space(val).to_string());
            }
            // id: / retry: 等字段目前无需处理
        }

        events
    }

    /// 刷出剩余缓冲中可能未被空行终结的尾部事件
    pub fn flush(&mut self) -> Option<SseEvent> {
        if self.data_lines.is_empty() && self.current_event.is_none() {
            return None;
        }
        let event = SseEvent {
            event: self.current_event.take(),
            data: self.data_lines.join("\n"),
        };
        self.data_lines.clear();
        Some(event)
    }

    fn find_line_end(&self) -> Option<usize> {
        self.buffer.find('\n').or_else(|| {
            // 单独的 \r 也是合法的行终结符（极少见）
            self.buffer.find('\r')
        })
    }
}

fn strip_leading_space(s: &str) -> &str {
    s.strip_prefix(' ').unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_sse_parsing() {
        let mut parser = SseParser::new();
        let events = parser.feed("event: message\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("message"));
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn multi_data_lines() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: line1\ndata: line2\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2");
    }

    #[test]
    fn cross_chunk_boundary() {
        let mut parser = SseParser::new();
        assert!(parser.feed("event: tes").is_empty());
        assert!(parser.feed("t\ndata: he").is_empty());
        let events = parser.feed("llo\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("test"));
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn openai_done_sentinel() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: [DONE]\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "[DONE]");
    }
}
