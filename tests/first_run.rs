//! What a fresh install says when there is no configuration to start from
//! ([#113](https://github.com/akiomik/nostui/issues/113)).
//!
//! Driven through the built binary rather than by calling `Config::new`: the directory
//! comes from `NOSTUI_CONFIG` through a `LazyLock` read once per process, so a unit test
//! setting that variable would race every other test in its binary for the first read.

use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::PathBuf;
use std::time::Duration;

use assert_cmd::Command;
use nostui::Result;
#[cfg(unix)]
use predicates::prelude::PredicateBooleanExt;
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
/// so these runs do not write into the data directory of whoever runs the tests. One per
/// case, because the cases in this file run as threads of one binary and the log is
/// truncated at every start.
fn data_dir(case: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("first-run-data-{case}"))
}

#[test]
fn a_missing_configuration_says_where_to_put_one_and_what_to_write_in_it() -> Result<()> {
    let config_dir = config_dir();

    // The log assertions at the end need the file this run writes, and two things stand
    // between them and it. `CARGO_TARGET_TMPDIR` outlives a run, so a previous green
    // run's log would be read in place of a missing one — which is the case most worth
    // catching, since a `NOSTUI_DATA` that stopped being honoured would send the binary
    // at the real data directory while every stderr assertion carried on passing. And
    // `initialize_logging` reads `RUST_LOG` ahead of `NOSTUI_LOGLEVEL` and its own
    // default, so what the runner exported decides whether anything is written at all.
    match fs::remove_dir_all(data_dir("missing")) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }

    Command::cargo_bin("nostui")?
        .timeout(RUN_TIMEOUT)
        .env("NOSTUI_CONFIG", &config_dir)
        .env("NOSTUI_DATA", data_dir("missing"))
        .env("RUST_LOG", "nostui=error")
        .assert()
        .failure()
        // No `NO_COLOR`: this report is `color_eyre`'s, which colours it either way, and
        // every substring below sits away from the escapes.
        .stderr(contains(config_dir.display().to_string()))
        // The whole phrase: `contains("config.json")` matches `config.json5` too.
        .stderr(contains("write config.json in it"))
        .stderr(contains("{\"key\": \"nsec1...\"}"))
        .stderr(contains("An npub instead of an nsec"));

    // The snippet rather than a line count, which would also fail for an unrelated event
    // logged before this one. It is a copy of prose in another file, but one the stderr
    // assertion above spells too — reworded, that fails first rather than this quietly
    // stopping.
    let log = fs::read_to_string(data_dir("missing").join("nostui.log"))?;

    assert!(
        log.contains(&config_dir.display().to_string()),
        "the log should name the directory, got: {log}"
    );
    assert!(
        !log.contains(r#"{"key": "nsec1..."}"#),
        "the instructions belong on the terminal, not in the log: {log}"
    );

    Ok(())
}

/// A working directory of this case's own, since a blank `NOSTUI_CONFIG` is what sends
/// the loader at the working directory — `CARGO_TARGET_TMPDIR` itself would be probed by
/// any case that later dropped a `config.*` beside it.
///
/// Canonical, because the child resolves `.` through `getcwd`, which follows symlinks
/// where `CARGO_TARGET_TMPDIR` does not: on a checkout reached through one, the two forms
/// name the same directory and do not compare equal.
#[cfg(unix)]
fn blank_config_cwd() -> Result<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("first-run-blank-cwd");
    fs::create_dir_all(&dir)?;

    Ok(dir.canonicalize()?)
}

/// `NOSTUI_CONFIG` set to nothing is set as far as `env::var` is concerned, so the loader
/// takes the empty path and joins each name onto it — probing the working directory. The
/// message has to say so: it once printed `No configuration file found in ` and left the
/// instruction under it pointing at nothing.
///
/// The case above cannot see this. It hands over an absolute directory, where
/// `path::absolute` changes nothing and the empty path never arises.
///
/// Unix only: `CreateProcess` drops an empty entry from the environment block, so on
/// Windows the variable would read as unset and the branch under test is unreachable
/// from here — the case would fail saying nothing about the guard it exists for.
#[cfg(unix)]
#[test]
fn a_blank_configuration_directory_names_the_working_directory_it_fell_back_to() -> Result<()> {
    let cwd = blank_config_cwd()?;

    Command::cargo_bin("nostui")?
        .timeout(RUN_TIMEOUT)
        .current_dir(&cwd)
        .env("NOSTUI_CONFIG", "")
        .env("NOSTUI_DATA", data_dir("blank"))
        .env("RUST_LOG", "nostui=error")
        .assert()
        .failure()
        .stderr(contains(cwd.display().to_string()));

    Ok(())
}

/// Opens the directory it is given when it goes out of scope, however that happens: an
/// assertion that panics must not leave one behind that `rm -rf target` and `cargo clean`
/// cannot get past.
#[cfg(unix)]
struct Reopened(PathBuf);

#[cfg(unix)]
impl Drop for Reopened {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, PermissionsExt::from_mode(0o755));
    }
}

/// A directory holding a configuration that this process cannot look inside. `exists`
/// answers `false` for it as readily as for an empty one.
///
/// Unix only, for `chmod`; and it asserts the directory really is shut rather than
/// skipping, so running as root fails here rather than passing having proved nothing.
#[cfg(unix)]
fn unreadable_config_dir() -> Result<Reopened> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("first-run-unreadable");

    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, PermissionsExt::from_mode(0o755))?;
    fs::write(dir.join("config.json"), r#"{"key": "nsec1..."}"#)?;
    fs::set_permissions(&dir, PermissionsExt::from_mode(0o000))?;

    // Before the assertion, which is the one thing here that can panic while the mode is
    // 000 — a user this does not keep out, which is the case it exists to catch.
    let shut = Reopened(dir);

    assert!(
        fs::read_dir(&shut.0).is_err(),
        "this case needs a user that a mode of 000 keeps out"
    );

    Ok(shut)
}

#[cfg(unix)]
#[test]
fn a_configuration_that_cannot_be_looked_at_is_not_reported_missing() -> Result<()> {
    let shut = unreadable_config_dir()?;
    let config_dir = &shut.0;

    Command::cargo_bin("nostui")?
        .timeout(RUN_TIMEOUT)
        .env("NOSTUI_CONFIG", config_dir)
        .env("NOSTUI_DATA", data_dir("unreadable"))
        .env("RUST_LOG", "nostui=error")
        .assert()
        .failure()
        .stderr(contains(config_dir.display().to_string()))
        // The directory, not a name inside it: every candidate failed here, so none of
        // them is the news. `contains(dir)` alone would pass on either.
        .stderr(contains(format!(
            "Could not look in {}:",
            config_dir.display()
        )))
        .stderr(contains("Make that directory").not());

    Ok(())
}

/// One candidate failing leaves the directory fine and the configuration still absent, so
/// the instructions are still what a reader needs — with the name that failed after them
/// rather than in their place.
#[cfg(unix)]
#[test]
fn a_candidate_that_cannot_be_looked_at_does_not_take_the_instructions_with_it() -> Result<()> {
    let config_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("first-run-looping");
    let looping = config_dir.join("config.json5");

    fs::create_dir_all(&config_dir)?;
    if looping.symlink_metadata().is_err() {
        symlink("config.json5", &looping)?;
    }

    Command::cargo_bin("nostui")?
        .timeout(RUN_TIMEOUT)
        .env("NOSTUI_CONFIG", &config_dir)
        .env("NOSTUI_DATA", data_dir("looping"))
        .env("RUST_LOG", "nostui=error")
        .assert()
        .failure()
        .stderr(contains("write config.json in it"))
        .stderr(contains(format!(
            "Could not look at {}:",
            looping.display()
        )));

    Ok(())
}
