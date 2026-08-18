//! Shared subprocess wrapper for M2/M3 tools.
//!
//! Every M2 tool spawns `apr <subcommand> [...args] --json` and passes stdout
//! through to the MCP client verbatim. Non-zero exit maps to `isError: true`
//! with stderr attached. This module centralizes that pattern so each tool is
//! a thin definition + a list of CLI args.
//!
//! M3 (FALSIFY-MCP-006) adds [`run_apr_cancellable`], which polls a
//! [`std::sync::mpsc::Receiver`] between `try_wait` checks and escalates to
//! SIGTERM → (grace window) → SIGKILL on the spawned subprocess when a
//! cancellation is signalled. The non-cancellable [`run_apr`] is kept as a
//! thin wrapper for tools that don't support cancellation yet.

use crate::types::ToolCallResult;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// Environment override naming the `apr` binary subprocess tools should drive.
pub const APR_BIN_ENV: &str = "APR_BIN";

/// Resolve the `apr` binary this server drives, given an explicit override and
/// the current executable path. Pure so the precedence is testable without
/// mutating process environment from parallel tests.
///
/// Precedence: `APR_BIN` → `current_exe()` **if it is itself an `apr`** → `"apr"` (PATH).
///
/// The `file_stem == "apr"` guard is load-bearing, not defensive dressing. This
/// module is linked into any binary that depends on the crate — most importantly
/// the unit-test harness, whose `current_exe()` is `aprender_mcp-<hash>`. Without
/// the guard, `run_apr` spawns *the test binary itself* with `apr`'s arguments;
/// the harness reads them as a test filter, runs nothing, and exits 0, so a
/// spawn-failure test silently reports success. Self-spawning is a worse failure
/// than the PATH bug it replaces, so anything not named `apr` falls back to PATH.
fn resolve_apr_binary(
    override_bin: Option<OsString>,
    current_exe: std::io::Result<PathBuf>,
) -> OsString {
    if let Some(explicit) = override_bin {
        if !explicit.is_empty() {
            return explicit;
        }
    }
    if let Ok(path) = current_exe {
        // `apr` on unix, `apr.exe` on Windows — both stem to "apr".
        if path.file_stem().is_some_and(|stem| stem == OsStr::new("apr")) {
            return path.into_os_string();
        }
    }
    // The OS could not tell us what we are, or we are not an `apr`.
    OsString::from("apr")
}

/// The `apr` binary to spawn.
///
/// The MCP server IS an `apr` subcommand (`apr mcp`), so the running executable
/// is by construction a correct, version-matched `apr`. Spawning a PATH-resolved
/// `"apr"` instead has two failure modes, both observed:
///
/// 1. `.mcp.json` configured with an absolute path (`{"command":"/path/to/apr",
///    "args":["mcp"]}`) leaves no `apr` on PATH, so every subprocess tool fails
///    with `Failed to spawn ...: No such file or directory`.
/// 2. When a *different* `apr` is on PATH, the server silently drives a binary
///    it is not — a v0.63.0 server shelling out to a stale build. Four `apr`
///    binaries have coexisted on one dev box (see CLAUDE.md "pin the binary").
#[must_use]
fn apr_binary() -> OsString {
    resolve_apr_binary(std::env::var_os(APR_BIN_ENV), std::env::current_exe())
}

/// Default grace window between SIGTERM and SIGKILL for cancelled calls.
///
/// Per `docs/specifications/apr-mcp-server-spec.md`:
/// > `notifications/cancelled` from client → kill the spawned `apr`
/// > subprocess with SIGTERM (30s grace) → SIGKILL.
pub const CANCEL_GRACE_MS: u64 = 30_000;

/// Poll interval when waiting for subprocess exit / cancel signal.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Spawn `apr <args...>` and wait synchronously. Shorthand for the
/// non-cancellable path used by every tool except `apr.run`.
///
/// - Successful exit with non-empty stdout → `success(stdout)`
/// - Successful exit with empty stdout → `error("apr ... produced no output")`
/// - Non-zero exit → `error("apr ... failed (exit N): <stderr-or-stdout>")`
/// - Spawn failure → `error("Failed to spawn apr ...: <io-err>")`
#[must_use]
pub fn run_apr(args: &[&str]) -> ToolCallResult {
    let program = apr_binary();
    let output = match Command::new(&program).args(args).output() {
        Ok(o) => o,
        Err(e) => {
            // Name the binary actually spawned, not the literal "apr". The old
            // message said `apr` regardless of what ran, which is precisely why
            // a PATH-resolution failure read as "apr is broken" instead of
            // "the server looked for apr somewhere you did not expect".
            let cmd = format!("{} {}", program.to_string_lossy(), args.join(" "));
            return ToolCallResult::error(format!("Failed to spawn `{cmd}`: {e}"));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if output.status.success() {
        if stdout.trim().is_empty() {
            let cmd = format!("apr {}", args.join(" "));
            ToolCallResult::error(format!("`{cmd}` produced no output"))
        } else {
            ToolCallResult::success(stdout)
        }
    } else {
        let code = output.status.code().unwrap_or(-1);
        let detail = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        let cmd = format!("apr {}", args.join(" "));
        ToolCallResult::error(format!("`{cmd}` failed (exit {code}): {detail}"))
    }
}

/// Spawn `apr <args...>` cancellable via `cancel_rx`.
///
/// On receipt of any value on `cancel_rx`, the subprocess is sent SIGTERM.
/// If it hasn't exited within `grace_ms` milliseconds, SIGKILL is sent. The
/// returned `ToolCallResult` carries whatever stdout was captured up to the
/// point of cancellation and has `is_error: Some(true)` with a message that
/// starts with `"Cancelled:"`.
///
/// Non-Unix targets do NOT support signalling; this function falls back to
/// `child.kill()` (equivalent to SIGKILL on Windows).
#[must_use]
pub fn run_apr_cancellable(
    args: &[&str],
    cancel_rx: &Receiver<()>,
    grace_ms: u64,
) -> ToolCallResult {
    spawn_cancellable(&apr_binary(), args, cancel_rx, grace_ms)
}

/// Test-visible generic over the binary name. `run_apr_cancellable` is the
/// `apr`-bound wrapper clients should use in production code.
#[must_use]
pub fn spawn_cancellable(
    program: impl AsRef<OsStr>,
    args: &[&str],
    cancel_rx: &Receiver<()>,
    grace_ms: u64,
) -> ToolCallResult {
    let program = program.as_ref();
    let cmd_display = format!("{} {}", program.to_string_lossy(), args.join(" "));

    let mut child = match Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return ToolCallResult::error(format!("Failed to spawn `{cmd_display}`: {e}"));
        }
    };

    let pid = child.id();

    // Poll loop: check if the child exited, then check for a cancel signal.
    // Sleep `POLL_INTERVAL` between iterations. This keeps cancel latency
    // under ~10ms while the subprocess is alive.
    let wait_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(e) => {
                return ToolCallResult::error(format!("Failed to poll `{cmd_display}`: {e}"));
            }
        }

        match cancel_rx.try_recv() {
            Ok(()) => break Err(CancelReason::Signalled),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                // Sender dropped without a cancel — treat as "no cancellation
                // will ever come" and just wait for natural exit.
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    };

    match wait_status {
        Ok(status) => {
            // Natural exit: drain pipes and map to success/error.
            let stdout = drain(&mut child.stdout.take());
            let stderr = drain(&mut child.stderr.take());
            if status.success() {
                if stdout.trim().is_empty() {
                    ToolCallResult::error(format!("`{cmd_display}` produced no output"))
                } else {
                    ToolCallResult::success(stdout)
                }
            } else {
                let code = status.code().unwrap_or(-1);
                let detail = if stderr.trim().is_empty() {
                    stdout
                } else {
                    stderr
                };
                ToolCallResult::error(format!("`{cmd_display}` failed (exit {code}): {detail}"))
            }
        }
        Err(CancelReason::Signalled) => {
            // SIGTERM, grace window, then SIGKILL.
            send_sigterm(pid);
            let deadline = Instant::now() + Duration::from_millis(grace_ms);
            let mut escalated = false;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {}
                    Err(_) => break,
                }
                if Instant::now() >= deadline {
                    if !escalated {
                        // Best-effort kill (SIGKILL on Unix, TerminateProcess
                        // on Windows). Ignore errors — the process may have
                        // exited between try_wait and here.
                        let _ = child.kill();
                        escalated = true;
                    } else {
                        // Even SIGKILL hasn't reaped it — give up after a
                        // short extra window to avoid hanging the main
                        // thread forever. In practice SIGKILL is immediate.
                        break;
                    }
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            // Reap if still alive (keeps us from leaking a zombie).
            let _ = child.wait();

            let stdout = drain(&mut child.stdout.take());
            let preview = truncate_for_preview(&stdout);
            ToolCallResult::error(format!(
                "Cancelled: `{cmd_display}` terminated by notifications/cancelled; partial stdout: {preview}"
            ))
        }
    }
}

enum CancelReason {
    Signalled,
}

fn drain<R: Read>(reader: &mut Option<R>) -> String {
    let mut buf = String::new();
    if let Some(r) = reader.as_mut() {
        let _ = r.read_to_string(&mut buf);
    }
    buf
}

fn truncate_for_preview(s: &str) -> String {
    const MAX: usize = 512;
    if s.len() <= MAX {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(MAX).collect();
        format!("{truncated}… (truncated)")
    }
}

#[cfg(unix)]
fn send_sigterm(pid: u32) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    // Cast is safe: u32 → i32 for PIDs in the valid pid_t range. Values
    // above i32::MAX would be invalid PIDs on every Unix we support, so
    // saturating is fine — `kill` will just fail with EINVAL and we move
    // on to the SIGKILL branch.
    #[allow(clippy::cast_possible_wrap)]
    let raw = pid as i32;
    let _ = kill(Pid::from_raw(raw), Signal::SIGTERM);
}

#[cfg(not(unix))]
fn send_sigterm(_pid: u32) {
    // Windows has no SIGTERM; we skip straight to the SIGKILL equivalent
    // (`child.kill()`) in the escalation path above.
}

/// Spawn `apr <args...>` and stream stdout line-by-line to `on_line`.
///
/// FALSIFY-MCP-PROGRESS-001: this is the streaming variant used by tools that
/// emit `notifications/progress` (currently `apr.finetune`). Each line of
/// stdout (as written by `apr <cmd> --json`) is passed to `on_line`
/// synchronously before the next `read_line` — the caller is responsible for
/// emitting the notification.
///
/// Returns a `ToolCallResult` whose body is the concatenated stdout (the same
/// shape as [`run_apr`] would have produced), so callers can keep the
/// existing "final payload" semantics while layering progress on top.
#[must_use]
pub fn run_apr_streaming<F>(args: &[&str], on_line: F) -> ToolCallResult
where
    F: FnMut(&str),
{
    spawn_streaming(&apr_binary(), args, on_line)
}

/// Generic-over-program variant of [`run_apr_streaming`] used by tests that
/// need to inject a mock subprocess.
#[must_use]
pub fn spawn_streaming<F>(program: impl AsRef<OsStr>, args: &[&str], mut on_line: F) -> ToolCallResult
where
    F: FnMut(&str),
{
    let program = program.as_ref();
    let cmd_display = format!("{} {}", program.to_string_lossy(), args.join(" "));

    let mut child = match Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return ToolCallResult::error(format!("Failed to spawn `{cmd_display}`: {e}"));
        }
    };

    // Take stdout so we can wrap it in a BufReader. Leaving stderr attached
    // to the child means we can drain it after wait() for the error path.
    let stdout_pipe = match child.stdout.take() {
        Some(p) => p,
        None => {
            let _ = child.wait();
            return ToolCallResult::error(format!("Failed to capture stdout of `{cmd_display}`"));
        }
    };

    let mut accumulated = String::new();
    let reader = BufReader::new(stdout_pipe);
    for line in reader.lines() {
        match line {
            Ok(text) => {
                on_line(&text);
                accumulated.push_str(&text);
                accumulated.push('\n');
            }
            Err(e) => {
                // Best-effort: surface the read error but still try to reap
                // the child so we don't leak a zombie.
                let _ = child.wait();
                return ToolCallResult::error(format!(
                    "Failed to read stdout of `{cmd_display}`: {e}"
                ));
            }
        }
    }

    // stdout closed → subprocess is either exited or about to. Wait for the
    // exit status so we can map success/failure correctly.
    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            return ToolCallResult::error(format!("Failed to reap `{cmd_display}`: {e}"));
        }
    };

    let stderr = drain(&mut child.stderr.take());

    if status.success() {
        if accumulated.trim().is_empty() {
            ToolCallResult::error(format!("`{cmd_display}` produced no output"))
        } else {
            ToolCallResult::success(accumulated)
        }
    } else {
        let code = status.code().unwrap_or(-1);
        let detail = if stderr.trim().is_empty() {
            accumulated
        } else {
            stderr
        };
        ToolCallResult::error(format!("`{cmd_display}` failed (exit {code}): {detail}"))
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // test helpers
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;

    /// An explicit `APR_BIN` override wins over everything else.
    #[test]
    fn resolve_apr_prefers_explicit_override() {
        let resolved = resolve_apr_binary(
            Some(OsString::from("/opt/custom/apr")),
            Ok(PathBuf::from("/usr/local/bin/apr")),
        );
        assert_eq!(resolved, OsString::from("/opt/custom/apr"));
    }

    /// With no override, the server drives ITSELF — the binary running
    /// `apr mcp` is by construction a correct `apr`. Regression guard: a bare
    /// `"apr"` here is the defect (every subprocess tool fails with "No such
    /// file or directory" when apr is not on PATH, and silently drives a
    /// DIFFERENT binary when a stale one is).
    #[test]
    fn resolve_apr_uses_current_exe_not_bare_path() {
        let resolved =
            resolve_apr_binary(None, Ok(PathBuf::from("/opt/aprender/target/release/apr")));
        assert_eq!(
            resolved,
            OsString::from("/opt/aprender/target/release/apr")
        );
        assert_ne!(resolved, OsString::from("apr"));
    }

    /// A `current_exe()` that is NOT an `apr` must never be spawned as one.
    /// Regression guard for a real failure: under `cargo test`, `current_exe()`
    /// is `aprender_mcp-<hash>`, and returning it made `run_apr` invoke the test
    /// harness with apr's arguments — the harness read them as a test filter and
    /// exited 0, so the spawn-failure test flipped from Some(true) to None.
    #[test]
    fn resolve_apr_refuses_a_current_exe_that_is_not_apr() {
        let resolved = resolve_apr_binary(
            None,
            Ok(PathBuf::from("/tmp/target/debug/deps/aprender_mcp-9f2b1c")),
        );
        assert_eq!(resolved, OsString::from("apr"));
    }

    /// Windows names it `apr.exe`; stripping the extension must still match.
    /// Uses a `/`-separated path deliberately: `PathBuf` applies *host* path
    /// semantics, so a literal `C:\tools\apr.exe` has no separators on unix and
    /// stems to `C:\tools\apr`, testing the harness rather than the code.
    #[test]
    fn resolve_apr_accepts_exe_suffix() {
        let resolved = resolve_apr_binary(None, Ok(PathBuf::from("/tools/apr.exe")));
        assert_eq!(resolved, OsString::from("/tools/apr.exe"));
    }

    /// Only when `current_exe()` is unavailable does PATH lookup apply.
    #[test]
    fn resolve_apr_falls_back_to_path_when_current_exe_fails() {
        let resolved = resolve_apr_binary(
            None,
            Err(std::io::Error::other("current_exe unavailable")),
        );
        assert_eq!(resolved, OsString::from("apr"));
    }

    /// Spawning `apr` with an unrecognised subcommand yields a tool error
    /// (non-zero exit), not a panic.
    #[test]
    fn spawn_failure_maps_to_tool_error() {
        let result = run_apr(&["this-subcommand-does-not-exist"]);
        assert_eq!(result.is_error, Some(true));
    }

    /// Cancellable path: a never-firing receiver lets the subprocess run to
    /// natural completion, producing identical behaviour to `run_apr`.
    #[test]
    fn cancellable_natural_exit_matches_run_apr() {
        let (_tx, rx) = mpsc::channel::<()>();
        let result = spawn_cancellable("echo", &["hello"], &rx, CANCEL_GRACE_MS);
        assert!(result.is_error.is_none(), "echo should succeed");
        assert!(result.content[0].text.contains("hello"));
    }

    /// Cancellable path: a disconnected receiver (sender dropped) is
    /// equivalent to "no cancellation will arrive" — behaviour should not
    /// change vs the never-firing channel.
    #[test]
    fn cancellable_disconnected_channel_is_noop() {
        let (tx, rx) = mpsc::channel::<()>();
        drop(tx);
        let result = spawn_cancellable("echo", &["world"], &rx, CANCEL_GRACE_MS);
        assert!(result.is_error.is_none());
        assert!(result.content[0].text.contains("world"));
    }

    /// Spawning a missing binary returns a spawn error without panic.
    #[test]
    fn cancellable_spawn_failure_maps_to_error() {
        let (_tx, rx) = mpsc::channel::<()>();
        let result = spawn_cancellable(
            "/this/binary/does/not/exist/apr-mcp-test",
            &[],
            &rx,
            CANCEL_GRACE_MS,
        );
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("Failed to spawn"));
    }

    /// FALSIFY-MCP-PROGRESS-001 (unit): `spawn_streaming` fires the callback
    /// once per stdout line before returning the aggregated payload.
    #[test]
    fn streaming_invokes_callback_per_line() {
        let lines = std::sync::Mutex::new(Vec::<String>::new());
        let result = spawn_streaming("printf", &["line1\nline2\nline3\n"], |line| {
            lines
                .lock()
                .expect("test mutex not poisoned")
                .push(line.to_string());
        });
        assert!(result.is_error.is_none(), "printf should succeed");

        let captured = lines.lock().expect("mutex").clone();
        assert_eq!(captured, vec!["line1", "line2", "line3"]);
        assert!(result.content[0].text.contains("line1"));
        assert!(result.content[0].text.contains("line3"));
    }

    /// Spawn failure in the streaming path returns a tool error without
    /// invoking the callback.
    #[test]
    fn streaming_spawn_failure_does_not_call_callback() {
        let called = std::sync::Mutex::new(false);
        let result = spawn_streaming(
            "/this/binary/does/not/exist/apr-mcp-streaming-test",
            &[],
            |_| {
                *called.lock().expect("mutex") = true;
            },
        );
        assert_eq!(result.is_error, Some(true));
        assert!(!*called.lock().expect("mutex"));
        assert!(result.content[0].text.contains("Failed to spawn"));
    }

    /// Streaming path: non-zero exit surfaces as a tool error.
    #[test]
    #[cfg(unix)]
    fn streaming_nonzero_exit_is_error() {
        let result = spawn_streaming("sh", &["-c", "echo partial; exit 3"], |_| {});
        assert_eq!(result.is_error, Some(true));
        assert!(
            result.content[0].text.contains("exit 3"),
            "message should include exit code: {}",
            result.content[0].text
        );
    }

    /// FALSIFY-MCP-006 (unit-level): sending a cancel signal to a
    /// long-running `sleep 60` subprocess returns within the grace window
    /// (SIGTERM is immediate for `sleep`, so we see natural reap well
    /// before the SIGKILL escalation).
    #[test]
    #[cfg(unix)]
    fn cancellable_stops_long_running_subprocess_within_grace() {
        let (tx, rx) = mpsc::channel::<()>();

        // Fire the cancel shortly after spawn to give the subprocess time
        // to get into its sleep syscall.
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            let _ = tx.send(());
        });

        let t0 = Instant::now();
        // Grace of 2s: if SIGTERM fails for any reason we still fall back
        // to SIGKILL well before the test's own timeout.
        let result = spawn_cancellable("sleep", &["60"], &rx, 2_000);
        let elapsed = t0.elapsed();

        handle.join().expect("cancel-sender thread joins");

        assert_eq!(result.is_error, Some(true), "cancelled calls are errors");
        assert!(
            result.content[0].text.starts_with("Cancelled:"),
            "message should indicate cancellation, got: {}",
            result.content[0].text
        );
        // 100ms fire + ~immediate SIGTERM response + cleanup — well under
        // the 2s grace + 200ms test slack the spec calls for.
        assert!(
            elapsed < Duration::from_millis(2_500),
            "cancel should finish within grace + slack, took {elapsed:?}"
        );
        // And it must finish meaningfully faster than the underlying
        // `sleep 60` would have — this is the real falsification.
        assert!(
            elapsed < Duration::from_secs(5),
            "cancelled call must return far before sleep 60's natural exit"
        );
    }
}
