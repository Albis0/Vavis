//! Starting helper processes without a console window.
//!
//! A release build has no console of its own (`windows_subsystem =
//! "windows"`), so every console program it starts -- PowerShell for
//! speech, media keys and typing, `cmd` for opening things, an MCP server
//! run with `node` -- gets a fresh console window, which flashes up in
//! front of whatever the user is doing and vanishes. Speech alone did it
//! once per sentence.
//!
//! Every spawn goes through [`hidden`]. It does nothing off Windows.

/// `CREATE_NO_WINDOW`: the child gets no console at all. Its standard
/// streams still work, which is all these helpers use.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Marks a command to start without a console window.
pub fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
