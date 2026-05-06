use std::collections::HashMap;
use std::io::{Read as IoRead, Write as IoWrite};
use std::sync::{Arc, Mutex};

use agentdash_relay::*;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use tokio::sync::mpsc;

/// 管理本机 PTY 会话生命周期。
///
/// 每个 `terminal_id` 对应一个 PTY 实例。写入由 relay 命令触发，
/// 读取由后台线程持续推送到 `event_tx`。
pub struct TerminalManager {
    /// 活跃的终端实例
    sessions: Arc<Mutex<HashMap<String, TerminalSession>>>,
    /// 向 relay WebSocket 发送事件的通道
    event_tx: mpsc::UnboundedSender<RelayMessage>,
}

struct TerminalSession {
    writer: Box<dyn IoWrite + Send>,
    /// 守护 kill 信号
    _reader_handle: tokio::task::JoinHandle<()>,
    master_pty: Box<dyn portable_pty::MasterPty + Send>,
}

impl TerminalManager {
    pub fn new(event_tx: mpsc::UnboundedSender<RelayMessage>) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub fn spawn(
        &self,
        payload: &TerminalSpawnPayload,
        workspace_root: &str,
    ) -> Result<TerminalSpawnResponse, String> {
        let pty_system = NativePtySystem::default();
        let size = PtySize {
            rows: payload.rows,
            cols: payload.cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(size)
            .map_err(|e| format!("PTY open failed: {e}"))?;

        let shell = payload.shell.clone().unwrap_or_else(|| {
            if cfg!(windows) {
                "powershell.exe".to_string()
            } else {
                std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
            }
        });

        let raw_cwd = payload
            .cwd
            .as_ref()
            .map(|c| {
                let p = std::path::Path::new(workspace_root).join(c);
                p.to_string_lossy().to_string()
            })
            .unwrap_or_else(|| workspace_root.to_string());

        // Windows canonicalize 会加 \\?\ 前缀，PowerShell 会原样显示导致提示符难看
        let cwd = strip_extended_length_prefix(&raw_cwd);

        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(&cwd);

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("PTY spawn failed: {e}"))?;

        let process_id = child.process_id();

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("PTY writer failed: {e}"))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("PTY reader failed: {e}"))?;

        let terminal_id = payload.terminal_id.clone();
        let event_tx = self.event_tx.clone();
        let sessions_ref = self.sessions.clone();

        // 发送 running 状态
        let _ = event_tx.send(RelayMessage::EventTerminalStateChanged {
            id: RelayMessage::new_id("term-state"),
            payload: TerminalStateChangedPayload {
                terminal_id: terminal_id.clone(),
                state: TerminalProcessState::Running,
                exit_code: None,
                message: None,
            },
        });

        // 读取线程：PTY → relay event
        let reader_terminal_id = terminal_id.clone();
        let reader_event_tx = event_tx.clone();
        let reader_sessions = sessions_ref.clone();

        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            tracing::info!(terminal_id = %reader_terminal_id, "PTY 读取线程启动");
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        tracing::info!(terminal_id = %reader_terminal_id, "PTY 读取结束 (EOF)");
                        break;
                    }
                    Ok(n) => {
                        tracing::debug!(terminal_id = %reader_terminal_id, bytes = n, "PTY 读取到数据");
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = reader_event_tx.send(RelayMessage::EventTerminalOutput {
                            id: RelayMessage::new_id("term-out"),
                            payload: TerminalOutputPayload {
                                terminal_id: reader_terminal_id.clone(),
                                data,
                            },
                        });
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, terminal_id = %reader_terminal_id, "PTY read ended");
                        break;
                    }
                }
            }

            // 等待子进程退出
            // PTY read 结束意味着子进程已退出
            reader_sessions.lock().unwrap().remove(&reader_terminal_id);

            let _ = reader_event_tx.send(RelayMessage::EventTerminalStateChanged {
                id: RelayMessage::new_id("term-state"),
                payload: TerminalStateChangedPayload {
                    terminal_id: reader_terminal_id,
                    state: TerminalProcessState::Exited,
                    exit_code: None,
                    message: None,
                },
            });
        });

        let session = TerminalSession {
            writer,
            _reader_handle: reader_handle,
            master_pty: pair.master,
        };

        self.sessions.lock().unwrap().insert(terminal_id, session);

        Ok(TerminalSpawnResponse {
            terminal_id: payload.terminal_id.clone(),
            process_id,
        })
    }

    pub fn input(&self, payload: &TerminalInputPayload) -> Result<TerminalInputResponse, String> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get_mut(&payload.terminal_id)
            .ok_or_else(|| format!("terminal not found: {}", payload.terminal_id))?;

        session
            .writer
            .write_all(payload.data.as_bytes())
            .map_err(|e| format!("write failed: {e}"))?;
        session
            .writer
            .flush()
            .map_err(|e| format!("flush failed: {e}"))?;

        Ok(TerminalInputResponse {
            terminal_id: payload.terminal_id.clone(),
        })
    }

    pub fn resize(
        &self,
        payload: &TerminalResizePayload,
    ) -> Result<TerminalResizeResponse, String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(&payload.terminal_id)
            .ok_or_else(|| format!("terminal not found: {}", payload.terminal_id))?;

        session
            .master_pty
            .resize(PtySize {
                rows: payload.rows,
                cols: payload.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("resize failed: {e}"))?;

        Ok(TerminalResizeResponse {
            terminal_id: payload.terminal_id.clone(),
        })
    }

    pub fn kill(&self, payload: &TerminalKillPayload) -> Result<TerminalKillResponse, String> {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.remove(&payload.terminal_id).is_some() {
            let _ = self.event_tx.send(RelayMessage::EventTerminalStateChanged {
                id: RelayMessage::new_id("term-state"),
                payload: TerminalStateChangedPayload {
                    terminal_id: payload.terminal_id.clone(),
                    state: TerminalProcessState::Killed,
                    exit_code: None,
                    message: Some("user requested kill".to_string()),
                },
            });
            Ok(TerminalKillResponse {
                terminal_id: payload.terminal_id.clone(),
                status: "killed".to_string(),
            })
        } else {
            Err(format!("terminal not found: {}", payload.terminal_id))
        }
    }
}

/// 去除 Windows 扩展路径前缀 `\\?\`，让 PowerShell 提示符显示正常路径。
fn strip_extended_length_prefix(path: &str) -> String {
    if cfg!(windows) {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    } else {
        path.to_string()
    }
}
