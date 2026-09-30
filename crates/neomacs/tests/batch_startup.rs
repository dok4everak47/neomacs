//! Batch startup error reporting, held against GNU's own oracle.
//!
//! GNU `src/keyboard.c:1174-1185` `top_level_2` pushes a `debug-early--handler`
//! binding around the top-level eval whenever `noninteractive`, so an uncaught
//! batch error prints a backtrace to `external-debugging-output` (stdout) while
//! the signalling frames are still live. GNU's acceptance tests for that are
//! `test/src/eval-tests.el:174-221`, and the three cases below are those tests,
//! run against this port instead of against `emacs`.
//!
//! Each case asserts exactly the three things GNU's own tests assert -- the
//! exit status, the presence or absence of a backtrace, and the message -- so
//! the expectations are GNU's acceptance criteria rather than a snapshot of one
//! build's wording.
//!
//! # Why these do not byte-compare against a live GNU
//!
//! They did, briefly, and it was wrong: this port tracks the pinned GNU 31.1
//! (`parity-reference.toml`), where `error` moved from a C subr to
//! `lisp/subr.el`'s `(signal 'error (list (apply #'format-message ...)))`, so a
//! batch backtrace legitimately carries one `signal(...)` frame that GNU 30.2's
//! C subr does not produce. Verified by giving GNU 30.2 the 31.1 definition of
//! `error`: its backtrace then matches this port's frame for frame. A byte
//! comparison against whatever `emacs` happens to be on `PATH` therefore scores
//! this port against the wrong reference -- the failure
//! `crates/neomacs-parity-reference` exists to prevent (ledger 214) -- and
//! `emacs` here is not the pin. GNU's own `search-forward`/`string-trim` form
//! is both the real oracle and version-portable.
//!
//! This runs the real executable, so it needs a release build with a matching
//! pdump (`NEOMACS_GUI_TEST_BINARY` overrides the path, as in
//! `main_stack_overflow.rs`).

use std::{path::PathBuf, process::Command};

fn root() -> PathBuf {
    neomacs_infra::crate_root!().join("../..")
}

fn binary() -> PathBuf {
    std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("target/release/neomacs"))
}

struct BatchRun {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run_batch(eval: &str) -> BatchRun {
    let output = Command::new(binary())
        .current_dir(root())
        .env("RUST_LOG", "off")
        .args(["-Q", "--batch", "--eval", eval])
        .output()
        .unwrap_or_else(|err| panic!("run {}: {err}", binary().display()));
    BatchRun {
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Run the port on `eval`, require it to fail, and require `message` on
/// stderr. GNU's `call-process` merges the two streams; the port's split is
/// asserted here instead, which is the stronger statement.
fn run_failing(eval: &str, message: &str) -> BatchRun {
    let port = run_batch(eval);
    assert_ne!(
        port.status,
        Some(0),
        "GNU treats a batch top-level error as fatal\nstdout:\n{}\nstderr:\n{}",
        port.stdout,
        port.stderr
    );
    assert!(
        port.stderr.contains(message),
        "the message is missing from stderr\nstderr:\n{}",
        port.stderr
    );
    port
}

/// GNU `test/src/eval-tests.el:174-190`
/// `eval-tests/backtrace-in-batch-mode`: an uncaught error in batch prints a
/// backtrace naming both the signalling function and `normal-top-level`.
#[test]
#[ignore = "requires release executable with matching pdump"]
fn batch_error_prints_gnus_backtrace() {
    let eval = "(progn (defun foo () (error \"Boo\")) (foo))";
    let port = run_failing(eval, "Boo");

    assert!(
        port.stdout.contains("  foo()"),
        "no backtrace frame for the signalling function\nstdout:\n{}",
        port.stdout
    );
    assert!(
        port.stdout.contains("  normal-top-level()"),
        "no backtrace frame for normal-top-level\nstdout:\n{}",
        port.stdout
    );
}

/// GNU `test/src/eval-tests.el:192-207`
/// `eval-tests/backtrace-in-batch-mode/inhibit`: binding
/// `backtrace-on-error-noninteractive` to nil suppresses it, and the output is
/// then exactly the message.
#[test]
#[ignore = "requires release executable with matching pdump"]
fn batch_error_backtrace_honours_inhibit() {
    let eval = "(progn (defun foo () (error \"Boo\")) (let ((backtrace-on-error-noninteractive nil)) (foo)))";
    let port = run_failing(eval, "Boo");

    assert!(
        port.stdout.is_empty(),
        "the inhibited backtrace was printed anyway\nstdout:\n{}",
        port.stdout
    );
}

/// GNU `test/src/eval-tests.el:209-221`
/// `eval-tests/backtrace-in-batch-mode/demoted-errors`: a *caught* error is not
/// a top-level error, so it produces neither a backtrace nor a failure exit.
#[test]
#[ignore = "requires release executable with matching pdump"]
fn batch_demoted_error_is_not_a_backtrace_case() {
    let eval = "(with-demoted-errors \"Error: %S\" (error \"Boo\"))";
    let port = run_batch(eval);

    assert_eq!(
        port.status,
        Some(0),
        "a caught error must not fail the session\nstdout:\n{}\nstderr:\n{}",
        port.stdout,
        port.stderr
    );
    assert!(
        !port.stdout.contains("  normal-top-level()"),
        "a caught error produced a backtrace\nstdout:\n{}",
        port.stdout
    );
    assert!(
        port.stderr.contains("Error: (error \"Boo\")"),
        "the demoted error was not reported\nstderr:\n{}",
        port.stderr
    );
}

/// Ledger 220's regression pin: a batch top-level error reports on stderr, and
/// `top_level_2`'s handler prints its backtrace on `stdout` while the frames
/// are still live -- without putting the error in `*Messages*`, which GNU owns
/// for the interactive path.
#[test]
#[ignore = "requires release executable with matching pdump"]
fn batch_error_exits_nonzero_and_reports_only_to_stderr() {
    let eval = "(progn (add-hook 'kill-emacs-hook (lambda () (princ (if (and (get-buffer \"*Messages*\") (with-current-buffer \"*Messages*\" (string-match-p \"batch-startup-probe-error\" (buffer-string)))) \"error-was-logged\" \"error-not-logged\")))) (error \"batch-startup-probe-error\"))";
    let port = run_failing(eval, "batch-startup-probe-error");

    assert!(
        port.stdout.contains("  normal-top-level()"),
        "the batch backtrace is missing\nstdout:\n{}",
        port.stdout
    );
    assert!(
        port.stdout.ends_with("error-not-logged"),
        "the error was written to *Messages*\nstdout:\n{}",
        port.stdout
    );
}

/// Ledger 220: a file load reports the error ONCE, no matter how deeply the
/// `load` calls nest.
///
/// `handler-bind` handlers keep running while the error propagates, but GNU
/// runs a given handler once per signal, not once per frame it unwinds
/// (`src/eval.c:1919-1934`: `call1 (h->val, error)` happens inside the
/// handler-list walk, and once a `CONDITION_CASE` clause matches the walk ends
/// at it). This port used to re-enter the walk at every `Flow -> EvalError ->
/// Flow` conversion, so `-l FILE` printed three backtraces -- three
/// `debug-early--handler` calls, the first with the full frame list and each
/// later one a shorter tail -- and the count grew with nesting: 3 at one
/// `load`, 7 at two, 9 at three, against GNU's 1 at every depth.
///
/// The fix carries the handler-search state (`SignalDispatchState`) across the
/// conversion, so the search is not restarted. This test pins the shallow case
/// and the depth independence together, because only the pair distinguishes
/// "runs once" from "runs once per level".
#[test]
#[ignore = "requires release executable with matching pdump"]
fn batch_load_reports_a_top_level_error_once_per_signal() {
    let dir = std::env::temp_dir().join("neomacs-batch-startup-load-depth");
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    let leaf = dir.join("leaf.el");
    let middle = dir.join("middle.el");
    let outer = dir.join("outer.el");
    std::fs::write(&leaf, "(error \"load-depth-probe\")\n").expect("write leaf");
    std::fs::write(&middle, format!("(load {:?})\n", leaf.to_string_lossy()))
        .expect("write middle");
    std::fs::write(&outer, format!("(load {:?})\n", middle.to_string_lossy()))
        .expect("write outer");

    // `-l` takes the same route `command-line-1` uses; the depth-3 chain is
    // the case that grew fastest while the search restarted.
    for (depth, file) in [("1", &leaf), ("2", &middle), ("3", &outer)] {
        let output = Command::new(binary())
            .current_dir(root())
            .env("RUST_LOG", "off")
            .args(["-Q", "--batch", "-l"])
            .arg(file)
            .output()
            .unwrap_or_else(|err| panic!("run {}: {err}", binary().display()));
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let blocks = stdout.matches("\nError: ").count();
        assert_eq!(
            blocks, 1,
            "depth {depth} printed {blocks} backtraces, GNU prints 1\nstdout:\n{stdout}"
        );
    }
}
