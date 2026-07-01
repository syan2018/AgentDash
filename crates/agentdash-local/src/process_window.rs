//! 子进程窗口策略。
//!
//! `agentdash-local` 既是 CLI，也是桌面壳托管的本机 runtime。CLI 自身保持 console
//! binary；由 runtime 自动拉起的非交互子进程在 Windows GUI 宿主下不应弹出控制台窗口。

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn hide_window_for_std_command(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

pub(crate) fn hide_window_for_tokio_command(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(windows))]
    {
        let _ = command;
    }
}
