pub mod keybindings;
pub mod styles;

use std::path::{self, Path, PathBuf};

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

/// Whether a format reads [`EXAMPLE_SNIPPET`]. YAML does,
/// being a superset of it here; TOML and INI want their own syntax, and a reader who
/// carried the snippet into one of those gets `missing configuration field "key"` — which
/// reads as having named the key wrong rather than having chosen the wrong file.
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
        let data_dir_str = data_dir.to_str().ok_or_else(|| {
            ConfigError::Message(format!("Data dir path is not valid UTF-8: {data_dir:?}"))
        })?;
        let config_dir_str = config_dir.to_str().ok_or_else(|| {
            ConfigError::Message(format!(
                "Config dir path is not valid UTF-8: {config_dir:?}"
            ))
        })?;
        let mut builder = config::Config::builder()
            .set_default("_data_dir", data_dir_str)?
            .set_default("_config_dir", config_dir_str)?;

        let mut found_config = false;
        for (file, format) in &CONFIG_FILES {
            builder = builder.add_source(
                config::File::from(config_dir.join(file))
                    .format(*format)
                    .required(false),
            );
            if config_dir.join(file).exists() {
                found_config = true
            }
        }
        if !found_config {
            // Where the loader looked, which is what it joins each name onto. A blank
            // `NOSTUI_CONFIG` leaves that empty and the names resolve against the working
            // directory, so that is the place to name — and it is what `path::absolute`
            // refuses to be asked about.
            let probed = if config_dir.as_os_str().is_empty() {
                Path::new(".")
            } else {
                config_dir.as_path()
            };

            // Absolute, because `get_config_dir` also answers `./.config` where there is
            // no home directory to ask about: a relative name means a different place
            // from every shell.
            let directory = path::absolute(probed).unwrap_or_else(|_| probed.to_path_buf());

            // The part a stranger cannot supply for themselves: it differs per platform,
            // and this is the whole of what a fresh install prints, with no window open
            // to read anything else in (#113).
            let found_nothing = format!("No configuration file found in {}", directory.display());

            // Split rather than listed together: the four are not interchangeable for the
            // snippet above them, and a line that named them as one would send a reader
            // to `config.ini` with JSON in it.
            let others = || {
                CONFIG_FILES
                    .iter()
                    .filter(|(file, _)| *file != EXAMPLE_FILE)
            };
            let names = |json: bool| {
                others()
                    .filter(move |(_, format)| reads_json(*format) == json)
                    .map(|(file, _)| *file)
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let (same_text, own_syntax) = (names(true), names(false));

            // Made as well as filled: `create_dir_all` is called for the data directory
            // and never for this one.
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
                // Which leaves it holding by construction for any `Ok` that arrives here,
                // since the guard above returns `NotFound` otherwise. What would catch
                // that guard going is a run against a configuration known to have no key,
                // which needs a directory of its own: #578.
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
                // The two ways `Config::new` refuses, in full rather than by a word of
                // them: no file at all, or one without a key, which arrives as
                // `NotFound("key")`. `contains("key")` would take almost any other error
                // with it, since `config` appends `` for key `…` `` to its own — a
                // `relays` written as a string passed here as an expected failure.
                let err_msg = format!("{e:?}");
                assert!(
                    err_msg.contains("No configuration file found")
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
    fn the_formats_the_error_says_take_the_snippet_take_it() -> Result<(), ConfigError> {
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

        Ok(())
    }

    /// Both lines that end the error name formats, and each needs one to name: the
    /// message introduces them as taking the same text or wanting their own, so a list
    /// that came out empty would introduce nothing at all, on every fresh install.
    #[test]
    fn the_error_has_a_format_to_name_on_each_of_its_last_two_lines() {
        let others = || {
            CONFIG_FILES
                .iter()
                .filter(|(file, _)| *file != EXAMPLE_FILE)
        };

        assert!(
            others().any(|(_, format)| reads_json(*format)),
            "the error says these take the same text as {EXAMPLE_FILE}, and names none"
        );
        assert!(
            others().any(|(_, format)| !reads_json(*format)),
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
