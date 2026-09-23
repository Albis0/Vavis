//! Running a helper process with a time limit.
//!
//! Every external program this crate starts -- PowerShell for Windows glue,
//! a shell for the code agent -- used to be waited on with `.output()`,
//! which waits forever. A command that stops to read input (`pause`,
//! `Read-Host`), opens a window and waits for it, or simply runs long held
//! the agent for as long as it ran: every later message refused, the tool
//! call never answered. Here the wait has an end, the process tree is
//! killed at it, and whatever was printed until then is kept.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What a run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// Exit code, or `None` if it was killed at the time limit.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Starts `cmd` hidden, with stdin closed, and waits at most `timeout`.
pub fn run(mut cmd: Command, timeout: Duration) -> Result<Run, String> {
    vavis_core::process::hidden(&mut cmd);
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("başlatılamadı: {e}"))?;

    // Drained on threads so a chatty process cannot fill a pipe and stall,
    // into buffers read at the end rather than joined: a grandchild that
    // outlives the killed parent holds the pipe open, and whatever was
    // printed before the kill is exactly what is worth showing.
    let drain = |mut pipe: Box<dyn Read + Send>| {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let sink = buf.clone();
        let done = Arc::new(AtomicBool::new(false));
        let finished = done.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            while let Ok(n) = pipe.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                sink.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .extend_from_slice(&chunk[..n]);
            }
            finished.store(true, Ordering::SeqCst);
        });
        (buf, done)
    };
    let (out_buf, out_done) = drain(Box::new(child.stdout.take().expect("piped")));
    let (err_buf, err_done) = drain(Box::new(child.stderr.take().expect("piped")));

    let deadline = Instant::now() + timeout;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status.code().unwrap_or(-1)),
            Ok(None) if Instant::now() >= deadline => {
                kill_tree(&mut child);
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(e.to_string()),
        }
    };
    // Let the readers catch the last of it; not forever.
    let grace = Instant::now() + Duration::from_secs(if code.is_some() { 5 } else { 1 });
    while !(out_done.load(Ordering::SeqCst) && err_done.load(Ordering::SeqCst))
        && Instant::now() < grace
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    let take = |b: &Arc<Mutex<Vec<u8>>>| {
        String::from_utf8_lossy(&b.lock().unwrap_or_else(|e| e.into_inner())).into_owned()
    };
    Ok(Run {
        code,
        stdout: take(&out_buf),
        stderr: take(&err_buf),
    })
}

/// Ends a process and, on Windows, everything it started: `cmd /C cargo
/// test` is three processes deep, and killing only `cmd` leaves the test
/// binary running.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let _ = vavis_core::process::hidden(&mut Command::new("taskkill"))
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn output_and_exit_code_come_back() {
        let mut c = Command::new("sh");
        c.args(["-c", "echo out; echo err >&2; exit 4"]);
        let r = run(c, Duration::from_secs(10)).unwrap();
        assert_eq!(r.code, Some(4));
        assert_eq!(r.stdout.trim(), "out");
        assert_eq!(r.stderr.trim(), "err");
    }

    #[test]
    fn a_process_that_waits_for_input_does_not_hang() {
        // stdin is closed, so `read` sees EOF instead of waiting.
        let mut c = Command::new("sh");
        c.args(["-c", "read x; echo got:$x"]);
        let r = run(c, Duration::from_secs(10)).unwrap();
        assert_eq!(r.code, Some(0));
    }

    #[test]
    fn a_process_past_its_time_is_killed_and_keeps_its_output() {
        let mut c = Command::new("sh");
        c.args(["-c", "echo early; sleep 10"]);
        let started = Instant::now();
        let r = run(c, Duration::from_millis(300)).unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(r.code, None);
        assert!(r.stdout.contains("early"));
    }
}
