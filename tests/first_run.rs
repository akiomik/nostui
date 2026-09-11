//! What a fresh install says when there is no configuration to start from.
//!
//! `Config::new` refuses to start without one, so this is the first thing a user who has
//! just unpacked a release sees — and there is no window open to read anything else in
//! ([#113](https://github.com/akiomik/nostui/issues/113)). What it has to carry is the
//! directory it looked in, which is the one part a user cannot guess: it differs per
//! platform, and the README can only list all three.
//!
//! Driven through the built binary rather than by calling `Config::new`, because the
//! directory comes from `NOSTUI_CONFIG` by way of a `LazyLock` that reads it once per
//! process. A unit test setting that variable would be racing every other test in its
//! binary for the first read.

use std::path::PathBuf;
use std::time::Duration;

use assert_cmd::Command;
use nostui::Result;
use predicates::str::contains;

/// Matches `tests/cli.rs`: nothing here should take a measurable amount of time, and a
/// regression that hangs is worth a failure rather than a silent `cargo test`.
const RUN_TIMEOUT: Duration = Duration::from_secs(30);

/// A directory with no configuration file in it, and never created — `Config::new` asks
/// whether each name exists, and a path that is not there answers no for all of them.
///
/// Named apart from `tests/cli.rs`'s rather than shared. `cargo test` runs the two
/// binaries one after another, so today they could share it — but nothing here rests on
/// that ordering, and `cargo nextest` does run them at once.
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

    Command::cargo_bin("nostui")?
        .timeout(RUN_TIMEOUT)
        .env("NOSTUI_CONFIG", &config_dir)
        .env("NOSTUI_DATA", data_dir())
        .assert()
        // Refusing to start is the existing behaviour and not what #113 is about: a
        // configuration is what the program needs to reach a relay at all, and starting
        // without one would trade this message for an empty timeline explaining nothing.
        .failure()
        // `NO_COLOR` is not set, unlike in `tests/cli.rs` where it silences clap: the
        // report here is `color_eyre`'s and comes out with its escape either way, which
        // I checked rather than assumed. Every substring below sits away from it.
        .stderr(contains(config_dir.display().to_string()))
        // The whole phrase, because `contains("config.json")` also matches `config.json5`
        // and so would pass with the JSON entry gone from the list entirely.
        .stderr(contains("write config.json in it"))
        .stderr(contains("{\"key\": \"nsec1...\"}"))
        // One alternative, named where it cannot be a prefix of another.
        .stderr(contains("config.toml"));

    Ok(())
}
