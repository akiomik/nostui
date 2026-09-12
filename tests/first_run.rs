//! What a fresh install says when there is no configuration to start from
//! ([#113](https://github.com/akiomik/nostui/issues/113)).
//!
//! Driven through the built binary rather than by calling `Config::new`: the directory
//! comes from `NOSTUI_CONFIG` through a `LazyLock` read once per process, so a unit test
//! setting that variable would race every other test in its binary for the first read.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use assert_cmd::Command;
use nostui::Result;
use predicates::str::contains;

/// Matches `tests/cli.rs`: nothing here should take a measurable amount of time, and a
/// regression that hangs is worth a failure rather than a silent `cargo test`.
const RUN_TIMEOUT: Duration = Duration::from_secs(30);

/// Never created: `Config::new` asks whether each name exists, and a path that is not
/// there answers no for all of them. Named apart from `tests/cli.rs`'s, which `cargo
/// nextest` runs at the same time as this one.
fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("first-run-config-that-is-not-there")
}

/// Somewhere for `initialize_logging` to open its file before the configuration is read,
/// so these runs do not write into the data directory of whoever runs the tests.
fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("first-run-data")
}

#[test]
fn a_missing_configuration_says_where_to_put_one_and_what_to_write_in_it() -> Result<()> {
    let config_dir = config_dir();

    // `CARGO_TARGET_TMPDIR` outlives the run, so without this the log read below is
    // whatever the last green run left. That matters for the one regression it cannot
    // otherwise see: were `NOSTUI_DATA` to stop being honoured, the binary would truncate
    // the log of whoever is running the tests, the stderr assertions would pass anyway,
    // and this would read the stale file and agree.
    match fs::remove_dir_all(data_dir()) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }

    Command::cargo_bin("nostui")?
        .timeout(RUN_TIMEOUT)
        .env("NOSTUI_CONFIG", &config_dir)
        .env("NOSTUI_DATA", data_dir())
        // `initialize_logging` takes this before `NOSTUI_LOGLEVEL` and before its own
        // default, so without it whatever the runner exported decides whether the log
        // read below is written at all — and an empty one passes the second assertion
        // having looked at nothing. `NO_COLOR` is deliberately absent: the report is
        // `color_eyre`'s, which emits its escape either way.
        .env("RUST_LOG", "nostui=error")
        .assert()
        .failure()
        .stderr(contains(config_dir.display().to_string()))
        // The whole phrase: `contains("config.json")` matches `config.json5` too.
        .stderr(contains("write config.json in it"))
        .stderr(contains("{\"key\": \"nsec1...\"}"))
        .stderr(contains("config.toml"));

    // An event is prefixed once however many lines it spans, so passing the whole message
    // to `log::error!` puts the instructions past the reach of a `grep` for ERROR. Counted
    // rather than matched on a phrase: the phrase would be a copy of prose in another file
    // and would stop asserting anything the moment that prose was reworded.
    let log = fs::read_to_string(data_dir().join("nostui.log"))?;

    assert!(
        log.contains(&config_dir.display().to_string()),
        "the log should name the directory, got: {log}"
    );
    assert_eq!(
        log.lines().count(),
        1,
        "the instructions belong on the terminal, not in the log: {log}"
    );

    Ok(())
}
