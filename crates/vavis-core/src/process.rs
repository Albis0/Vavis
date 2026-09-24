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

/// Text as a PowerShell single-quoted literal, quotes included.
///
/// Doubling `'` is not enough. PowerShell also ends a single-quoted string
/// at `‘ ’ ‚ ‛`, the typographic quotes -- so "it’s", written the way most
/// text writes it, broke every script it reached, and `x’; Remove-Item …;’`
/// ran the command in the middle. Each of them is doubled here, which
/// PowerShell reads back as the one character.
///
/// Every piece of outside text that goes into a script -- a window title,
/// a sentence to speak, a control's name, a file path -- goes through this.
pub fn ps_quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_single_quote_is_doubled() {
        assert_eq!(ps_quote("it's"), "'it''s'");
        assert_eq!(ps_quote("it\u{2019}s"), "'it\u{2019}\u{2019}s'");
        assert_eq!(
            ps_quote("\u{2018}\u{201A}\u{201B}"),
            "'\u{2018}\u{2018}\u{201A}\u{201A}\u{201B}\u{201B}'"
        );
    }

    #[test]
    fn nothing_else_is_touched() {
        // Double quotes, `$` and backticks mean nothing inside single quotes.
        assert_eq!(ps_quote("\"$env:X\" `n ş"), "'\"$env:X\" `n ş'");
        assert_eq!(ps_quote(""), "''");
    }
}
