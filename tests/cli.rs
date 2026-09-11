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
use std::time::Duration;

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

/// Nothing here should take a measurable amount of time; this is the difference between
/// a regression that fails and one that hangs `cargo test` with no output.
///
/// What would hang: the only reason a run that gets past argument handling stops is that
/// `Config::new` refuses to start, which is an invariant of another module. Were
/// configuration to become optional, `ratatui::init()` would follow, and crossterm opens
/// `/dev/tty` rather than the piped stdout — so on a developer's machine it would
/// succeed, take the real terminal, and wait for input forever.
const RUN_TIMEOUT: Duration = Duration::from_secs(30);

fn nostui() -> Result<Command> {
    let mut command = Command::cargo_bin("nostui")?;
    command
        .timeout(RUN_TIMEOUT)
        .env("NOSTUI_CONFIG", config_dir())
        .env("NOSTUI_DATA", data_dir())
        // clap styles the words asserted below, and it colours a pipe too when the
        // caller exports `CLICOLOR_FORCE`. `anstream` reads `NO_COLOR` before that one,
        // so this is the whole of it: nothing here should turn red over an environment
        // the change under test has nothing to do with.
        .env("NO_COLOR", "1");
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
    let expected_data_dir = data_dir().display().to_string();

    nostui()?
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(format!("v{}", env!("CARGO_PKG_VERSION"))))
        // Both directories it names are the ones it was told to use, which is also the
        // only thing standing between these runs and the real ones. The data directory
        // needs saying as much as the config one: `initialize_logging` runs before the
        // parse and truncates the log file it opens, so an env key that stopped working
        // would empty the log of whoever ran the tests, quietly.
        .stdout(contains(format!("Config directory: {expected_config_dir}")))
        .stdout(contains(format!("Data directory: {expected_data_dir}")));

    Ok(())
}
