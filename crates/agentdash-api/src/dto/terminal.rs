use serde::Deserialize;

#[derive(Deserialize)]
pub struct SpawnTerminalBody {
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

#[derive(Deserialize)]
pub struct TerminalInputBody {
    pub data: String,
}

#[derive(Deserialize)]
pub struct TerminalResizeBody {
    pub cols: u16,
    pub rows: u16,
}
