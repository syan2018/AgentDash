use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct ResourceServer {
    base_url: String,
    roots: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl ResourceServer {
    pub fn start() -> std::io::Result<Self> {
        let std_listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        std_listener.set_nonblocking(true)?;
        let addr = std_listener.local_addr()?;
        let listener = TcpListener::from_std(std_listener)?;
        let server = Self {
            base_url: format!("http://127.0.0.1:{}", addr.port()),
            roots: Arc::new(RwLock::new(HashMap::new())),
        };
        tokio::spawn(run_server(listener, server.roots.clone()));
        Ok(server)
    }

    pub async fn register_root(&self, token: String, root: PathBuf) -> String {
        self.roots.write().await.insert(token.clone(), root);
        format!("{}/materialized/{}", self.base_url, token)
    }
}

async fn run_server(listener: TcpListener, roots: Arc<RwLock<HashMap<String, PathBuf>>>) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let roots = roots.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_connection(stream, roots).await {
                        tracing::debug!(error = %error, "materialized resource request failed");
                    }
                });
            }
            Err(error) => {
                tracing::warn!(error = %error, "materialized resource server accept failed");
                break;
            }
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    roots: Arc<RwLock<HashMap<String, PathBuf>>>,
) -> std::io::Result<()> {
    let mut buffer = vec![0_u8; 8192];
    let read = tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut buffer))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "request timeout"))??;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let Some(first_line) = request.lines().next() else {
        return write_response(&mut stream, 400, "text/plain", b"bad request").await;
    };
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    if method != "GET" && method != "HEAD" {
        return write_response(&mut stream, 405, "text/plain", b"method not allowed").await;
    }

    let Some((token, relative)) = parse_materialized_path(path) else {
        return write_response(&mut stream, 404, "text/plain", b"not found").await;
    };
    let Some(root) = roots.read().await.get(&token).cloned() else {
        return write_response(&mut stream, 404, "text/plain", b"not found").await;
    };
    let relative_path = match safe_relative_path(&relative) {
        Ok(path) => path,
        Err(_) => return write_response(&mut stream, 400, "text/plain", b"bad path").await,
    };
    let target = root.join(relative_path);
    if !target.starts_with(&root) {
        return write_response(&mut stream, 400, "text/plain", b"bad path").await;
    }

    if target.is_dir() {
        let body = directory_listing(&target, path).await?;
        return write_response(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            body.as_bytes(),
        )
        .await;
    }
    if !target.is_file() {
        return write_response(&mut stream, 404, "text/plain", b"not found").await;
    }
    let bytes = tokio::fs::read(&target).await?;
    let content_type = content_type_for(&target);
    if method == "HEAD" {
        write_head_response(&mut stream, 200, content_type, bytes.len()).await
    } else {
        write_response(&mut stream, 200, content_type, &bytes).await
    }
}

fn parse_materialized_path(path: &str) -> Option<(String, String)> {
    let path = path.split('?').next().unwrap_or(path);
    let rest = path.strip_prefix("/materialized/")?;
    let (token, relative) = rest.split_once('/').unwrap_or((rest, ""));
    if token.is_empty()
        || !token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return None;
    }
    Some((token.to_string(), percent_decode(relative)?))
}

fn safe_relative_path(raw: &str) -> Result<PathBuf, ()> {
    let mut path = PathBuf::new();
    if raw.is_empty() {
        return Ok(path);
    }
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(segment) => path.push(segment),
            Component::CurDir => {}
            _ => return Err(()),
        }
    }
    Ok(path)
}

async fn directory_listing(path: &Path, request_path: &str) -> std::io::Result<String> {
    let mut entries = tokio::fs::read_dir(path).await?;
    let mut rows = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        let suffix = if entry.file_type().await?.is_dir() {
            "/"
        } else {
            ""
        };
        let href = format!(
            "{}/{}{}",
            request_path.trim_end_matches('/'),
            url_encode_path_segment(&name),
            suffix
        );
        rows.push(format!(
            "<li><a href=\"{}\">{}{}</a></li>",
            html_escape(&href),
            html_escape(&name),
            suffix
        ));
    }
    rows.sort();
    Ok(format!(
        "<!doctype html><meta charset=\"utf-8\"><title>materialized resource</title><ul>{}</ul>",
        rows.join("")
    ))
}

async fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    write_head_response(stream, status, content_type, body.len()).await?;
    stream.write_all(body).await
}

async fn write_head_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    content_length: usize,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        status, reason, content_type, content_length
    );
    stream.write_all(headers.as_bytes()).await
}

fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = *bytes.get(i + 1)?;
            let lo = *bytes.get(i + 2)?;
            out.push((hex_value(hi)? << 4) | hex_value(lo)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn url_encode_path_segment(input: &str) -> String {
    let mut out = String::new();
    for byte in input.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
