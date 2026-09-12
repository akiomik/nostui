pub mod keybindings;
pub mod styles;

use std::path::PathBuf;

use color_eyre::eyre::Result;
use config::ConfigError;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use crate::utils;

const CONFIG: &str = include_str!("../../.config/config.json5");

/// The names `Config::new` reads, each with the format it is parsed as. Later entries
/// layer over earlier ones, which is `config`'s doing rather than a choice made here.
const CONFIG_FILES: [(&str, config::FileFormat); 5] = [
    ("config.json5", config::FileFormat::Json5),
    ("config.json", config::FileFormat::Json),
    ("config.yaml", config::FileFormat::Yaml),
    ("config.toml", config::FileFormat::Toml),
    ("config.ini", config::FileFormat::Ini),
];

/// The one the error names, and the one the README tells people to write. It has to be in
/// [`CONFIG_FILES`] under a format that reads the snippet shown beside it — not JSON in
/// particular, since `config.json5` and `config.yaml` read it too.
const EXAMPLE_FILE: &str = "config.json";

/// The least a configuration can hold, and what the error shows beside [`EXAMPLE_FILE`].
const EXAMPLE_SNIPPET: &str = r#"{"key": "nsec1..."}"#;

/// The names in [`CONFIG_FILES`] the error offers as alternatives: all of them but the
/// one it tells a reader to write. Defined once so the message and the tests over it
/// cannot come to disagree about which names that leaves.
fn alternatives() -> impl Iterator<Item = (&'static str, config::FileFormat)> {
    CONFIG_FILES
        .into_iter()
        .filter(|(file, _)| *file != EXAMPLE_FILE)
}

/// Whether a format reads [`EXAMPLE_SNIPPET`]. YAML does, being a superset of it here;
/// TOML and INI want their own syntax.
const fn reads_json(format: config::FileFormat) -> bool {
    matches!(
        format,
        config::FileFormat::Json | config::FileFormat::Json5 | config::FileFormat::Yaml
    )
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub _data_dir: PathBuf,
    #[serde(default)]
    pub _config_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Nip38Config {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default, flatten)]
    pub config: AppConfig,
    #[serde(default)]
    pub keybindings: keybindings::KeyBindings,
    #[serde(default)]
    pub styles: styles::Styles,
    #[serde(default)]
    #[warn(deprecated)]
    pub privatekey: SecretString,
    #[serde(default)]
    pub key: SecretString,
    #[serde(default)]
    pub relays: Vec<String>,
    #[serde(default, rename = "nip-38")]
    pub nip38: Nip38Config,
}

impl Config {
    pub fn new() -> Result<Self, config::ConfigError> {
        let default_config: Config = json5::from_str(CONFIG)
            .map_err(|e| ConfigError::Message(format!("Failed to load default config: {e}")))?;
        let data_dir = utils::get_data_dir();
        let config_dir = utils::get_config_dir();
        // Lossy rather than refused: these two reach nothing but `config`'s own defaults,
        // which nothing here reads back, and a directory whose name is not UTF-8 is no
        // reason to refuse to start when the configuration in it is readable. Since
        // `get_config_dir` began answering with an absolute path, the bytes being asked
        // about are the working directory's as well as the configured name's.
        let data_dir_str = data_dir.to_string_lossy();
        let config_dir_str = config_dir.to_string_lossy();

        let mut builder = config::Config::builder()
            .set_default("_data_dir", data_dir_str.as_ref())?
            .set_default("_config_dir", config_dir_str.as_ref())?;

        // `try_exists` rather than `exists`, which answers `false` for a directory it
        // cannot look inside as readily as for one holding nothing. The message below
        // would then tell someone to create a directory that is already there and write a
        // file they already have.
        let mut found_config = false;
        let mut unreadable = None;
        for (file, format) in &CONFIG_FILES {
            let path = config_dir.join(file);

            builder = builder.add_source(
                config::File::from(path.clone())
                    .format(*format)
                    .required(false),
            );
            match path.try_exists() {
                Ok(true) => found_config = true,
                Ok(false) => {}
                Err(e) => unreadable = unreadable.or(Some(e)),
            }
        }
        if let (false, Some(e)) = (found_config, unreadable) {
            let message = format!(
                "Could not look for a configuration in {}: {e}",
                config_dir.display()
            );

            log::error!("{message}");
            return Err(ConfigError::Message(message));
        }
        if !found_config {
            // The part a stranger cannot supply for themselves: it differs per platform,
            // and this is the whole of what a fresh install prints, with no window open
            // to read anything else in (#113).
            let found_nothing = format!("No configuration file found in {}", config_dir.display());

            // Split rather than listed together: the four are not interchangeable for the
            // snippet above them, and a line that named them as one would send a reader
            // to `config.ini` with JSON in it.
            let names = |json: bool| {
                alternatives()
                    .filter(move |(_, format)| reads_json(*format) == json)
                    .map(|(file, _)| file)
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let (same_text, own_syntax) = (names(true), names(false));

            // Made as well as filled: nothing creates it as the config directory. Where a
            // platform puts the data directory in the same place — macOS does —
            // `initialize_logging` has already made it, which is what the hedge is for.
            let message = format!(
                "{found_nothing}\n\
                 Make that directory if it is not there, then write {EXAMPLE_FILE} in it:\n\
                 \x20   {EXAMPLE_SNIPPET}\n\
                 An npub instead of an nsec starts nostui read-only.\n\
                 These take the same text: {same_text}\n\
                 These want their own syntax: {own_syntax}"
            );

            // The first line only; `tests/first_run.rs` holds the log to it.
            log::error!("{found_nothing}");
            return Err(ConfigError::Message(message));
        }

        let mut cfg: Self = builder.build()?.try_deserialize()?;

        // Merge default keybindings into user config (screen-specific mapping)
        for (keyseq, action) in default_config.keybindings.home.iter() {
            cfg.keybindings
                .home
                .entry(keyseq.clone())
                .or_insert_with(|| action.clone());
        }
        for (style_key, style) in default_config.styles.iter() {
            cfg.styles
                .entry(style_key.clone())
                .or_insert_with(|| *style);
        }

        if cfg.privatekey.expose_secret().is_empty() && cfg.key.expose_secret().is_empty() {
            return Err(ConfigError::NotFound("key".to_owned()));
        }

        if cfg.relays.is_empty() {
            cfg.relays.clone_from(&default_config.relays);
        }

        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_new_either_loads_a_key_and_relays_or_says_what_is_missing() {
        // This test needs to be updated to work in an environment where config files exist
        // For now, let's test that Config::new() either succeeds or fails for expected reasons
        match Config::new() {
            Ok(cfg) => {
                // If config loads successfully, it should have required fields
                println!("Config loaded successfully in test environment");
                // The same pair `Config::new` accepts, rather than one of the two: the
                // error it prints tells people to write `key`, so a contributor whose own
                // configuration takes that advice would otherwise fail this.
                //
                // Both halves need the environment to supply the case. Deleting the guard
                // above fails this one, but only where the configuration has no key;
                // emptying the bundled `.config/config.json5` fails the relays one below.
                // A run against a directory this test owns is what would stop that being
                // luck: #578.
                assert!(
                    !cfg.privatekey.expose_secret().is_empty()
                        || !cfg.key.expose_secret().is_empty(),
                    "a configuration that loads has one of the two key fields set"
                );
                assert!(!cfg.relays.is_empty(), "relays should not be empty");
            }
            Err(e) => {
                // If it fails, it should be for expected reasons (no config file or no privatekey)
                println!("Config failed as expected: {e:?}");
                // The three ways `Config::new` refuses, in full rather than by a word of
                // them: nothing found, nothing readable, or nothing holding a key — the
                // last arriving as `NotFound("key")`. `contains("key")` would take almost any other error
                // with it, since `config` appends `` for key `…` `` to its own — a
                // `relays` written as a string passed here as an expected failure.
                let err_msg = format!("{e:?}");
                assert!(
                    err_msg.contains("No configuration file found")
                        || err_msg.contains("Could not look for a configuration")
                        || err_msg.contains(r#"missing configuration field "key""#),
                    "an error here should name what is missing, got: {e:?}",
                );
            }
        }
    }

    /// The error tells a reader which of the names take the text it shows and which want
    /// their own, and [`reads_json`] is the only thing that decides. Asked of the parsers
    /// rather than of a second copy of the predicate: what the message promises is that
    /// this snippet, in a file of that name, is read.
    #[test]
    fn the_formats_the_error_says_take_the_snippet_take_it() {
        for (name, format) in CONFIG_FILES {
            let read = config::Config::builder()
                .add_source(config::File::from_str(EXAMPLE_SNIPPET, format))
                .build()
                .and_then(|parsed| parsed.get_string("key"));

            assert_eq!(
                read.is_ok(),
                reads_json(format),
                "{name} reads the snippet: {}, and the error says {}",
                read.is_ok(),
                reads_json(format)
            );
        }
    }

    /// Both lines that end the error name formats, and each needs one to name: the
    /// message introduces them as taking the same text or wanting their own, so a list
    /// that came out empty would introduce nothing at all, on every fresh install.
    #[test]
    fn the_error_has_a_format_to_name_on_each_of_its_last_two_lines() {
        assert!(
            alternatives().any(|(_, format)| reads_json(format)),
            "the error says these take the same text as {EXAMPLE_FILE}, and names none"
        );
        assert!(
            alternatives().any(|(_, format)| !reads_json(format)),
            "the error says these want their own syntax, and names none"
        );
    }

    /// The error tells a user to write one particular file and shows JSON to put in it.
    /// Nothing else checks either half: the name is spelled once and only filtered out of
    /// the list, so renaming the JSON entry would leave the message naming a file the
    /// program ignores, and re-pointing it at another format would leave it telling every
    /// fresh install to write JSON somewhere parsed as TOML. Both with the suite green.
    ///
    /// The formats accepted are the ones that read the snippet, which is what
    /// [`EXAMPLE_FILE`]'s own documentation says: YAML among them, since it is a JSON
    /// superset here. Narrowing this to JSON would fail a `config.yaml` the message
    /// would be right about.
    #[test]
    fn the_error_names_a_file_that_reads_the_json_it_shows() {
        assert!(
            CONFIG_FILES
                .iter()
                .any(|(file, format)| *file == EXAMPLE_FILE && reads_json(*format)),
            "the error shows JSON beside {EXAMPLE_FILE}, which {CONFIG_FILES:?} does not read"
        );
    }
}
