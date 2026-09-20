//! Keeping the helper programs out of sight on Windows.
//!
//! A GUI application has no console of its own, so Windows gives every console
//! program it starts a brand new console window: MediaMTX would sit behind the
//! app in a black window for the whole session, and the `tasklist` polls would
//! flash one every few seconds. `CREATE_NO_WINDOW` suppresses that.
//!
//! `hide` is called unconditionally and does nothing on macOS. `hide_std` only
//! exists on Windows, because every caller of it is a Windows-only module.

/// <https://learn.microsoft.com/windows/win32/procthread/process-creation-flags>
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn hide(command: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(windows)]
pub(crate) fn hide_std(command: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;

    command.creation_flags(CREATE_NO_WINDOW)
}
