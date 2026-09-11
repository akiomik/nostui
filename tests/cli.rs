//! What the one `Cli` parse in `tokio_main` buys, asserted against the built binary.
//!
//! `Cli` has no fields — `--tick-rate` was the last one and #527 removed it — so the
//! call that parses it binds nothing and returns a value with nothing in it:
//!
//! ```ignore
//! let _ = <Cli as Parser>::parse();
//! ```
//!
//! That one line is the whole of `nostui --help`, `nostui --version`, and `nostui`
//! refusing an argument it does not know. A tidy-up would delete it and leave
//! `nostui --bogus` starting the application instead of saying no. A unit test over
//! `Cli::try_parse_from` would not notice: it asserts that clap rejects the argument,
//! which clap does whether or not anything calls it (#533).
//!
//! So these run the binary. Each one fails if that line goes: without it the arguments
//! reach nothing that reads them, the program carries on to load a configuration that
//! is not there, and exits 1 saying something went wrong.

use std::path::PathBuf;

use assert_cmd::Command;
use nostui::Result;
use predicates::str::contains;

/// A directory with no configuration file in it, which is the point: `Config::new`
/// refuses to start without one, so a run that gets past argument handling ends in a
/// diagnosable failure rather than a terminal this test cannot drive.
fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli-config-that-is-not-there")
}

/// Somewhere to put the log file `initialize_logging` opens before the parse, so these
/// runs do not write into the data directory of whoever runs the tests.
fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli-data")
}

fn nostui() -> Result<Command> {
    let mut command = Command::cargo_bin("nostui")?;
    command
        .env("NOSTUI_CONFIG", config_dir())
        .env("NOSTUI_DATA", data_dir());
    Ok(command)
}

#[test]
fn an_unknown_argument_is_refused_rather_than_ignored() -> Result<()> {
    nostui()?
        .arg("--bogus")
        .assert()
        // clap's own exit code, and its own words: reaching the application instead
        // would exit 1 complaining about the configuration, which is the failure this
        // test exists to tell apart from a refusal.
        .code(2)
        .stderr(contains("unexpected argument '--bogus'"));

    Ok(())
}

#[test]
fn help_is_printed_and_the_application_does_not_start() -> Result<()> {
    nostui()?
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Usage: nostui"))
        .stdout(contains("--version"));

    Ok(())
}

#[test]
fn version_is_printed_with_the_directories_it_would_use() -> Result<()> {
    let expected_config_dir = config_dir().display().to_string();

    nostui()?
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(format!("v{}", env!("CARGO_PKG_VERSION"))))
        // The directory it names is the one it was told to use, which is also what
        // keeps these runs out of the real one.
        .stdout(contains(format!("Config directory: {expected_config_dir}")));

    Ok(())
}
