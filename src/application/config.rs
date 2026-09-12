pub mod keybindings;
pub mod styles;

use std::path::PathBuf;

use color_eyre::eyre::Result;
use config::ConfigError;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use crate::utils;

const CONFIG: &str = include_str!("../../.config/config.json5");

const CONFIG_FILES: [(&str, config::FileFormat); 5] = [
    ("config.json5", config::FileFormat::Json5),
    ("config.json", config::FileFormat::Json),
    ("config.yaml", config::FileFormat::Yaml),
    ("config.toml", config::FileFormat::Toml),
    ("config.ini", config::FileFormat::Ini),
];

/// The file the no-configuration error tells a reader to write, and what to put in it.
const EXAMPLE_FILE: &str = "config.json";
const EXAMPLE_SNIPPET: &str = r#"{"key": "nsec1..."}"#;

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
        // Lossy rather than refused: nothing reads these back, and a directory name that
        // is not UTF-8 is no reason not to start.
        let data_dir_str = data_dir.to_string_lossy();
        let config_dir_str = config_dir.to_string_lossy();

        let mut builder = config::Config::builder()
            .set_default("_data_dir", data_dir_str.as_ref())?
            .set_default("_config_dir", config_dir_str.as_ref())?;

        // `try_exists`, because `exists` answers `false` for a directory it cannot look
        // inside as readily as for one holding nothing.
        let mut found_config = false;
        let mut unreadable = Vec::new();
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
                Err(e) => unreadable.push((path, e)),
            }
        }
        if !found_config {
            let found_nothing = format!("No configuration file found in {}", config_dir.display());

            // Every name failing says nothing about any one of them: what they have in
            // common is the directory, and telling someone to make one that is already
            // there helps nobody. Some failing leaves the directory fine and the
            // instructions worth having, with the name that failed alongside them.
            let message = match unreadable.first() {
                Some((_, e)) if unreadable.len() == CONFIG_FILES.len() => {
                    format!("Could not look in {}: {e}", config_dir.display())
                }
                rest => {
                    let mut message = format!(
                        "{found_nothing}\n\
                         Make that directory if it is not there, then write {EXAMPLE_FILE} in it:\n\
                         \x20   {EXAMPLE_SNIPPET}\n\
                         An npub instead of an nsec starts nostui read-only."
                    );

                    if let Some((path, e)) = rest {
                        message.push_str(&format!("\nCould not look at {}: {e}", path.display()));
                    }

                    message
                }
            };

            // A `tracing` event is prefixed once however many lines it spans, so the rest
            // would be out of reach of a `grep` for ERROR.
            let first_line = message.lines().next().unwrap_or(&message);

            log::error!("{first_line}");
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
                // The pair `Config::new` accepts, not one of the two: the error tells
                // people to write `key`. Both assertions here need the environment to
                // supply their case, which #578 would settle.
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
                // In full rather than by a word: `contains("key")` also matches the
                // `` for key `…` `` that `config` appends to errors of its own.
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

    /// The message names one file and shows one text. Nothing else holds the two
    /// together, and holding them means deserialising: a `rename` on `Config::key` leaves
    /// the snippet parsing and the key unbound, which every fresh install would meet as
    /// `missing configuration field "key"` after following the instructions exactly.
    #[test]
    fn the_file_the_error_names_gives_nostui_the_key_it_shows() {
        let format = CONFIG_FILES
            .iter()
            .find_map(|(file, format)| (*file == EXAMPLE_FILE).then_some(*format))
            .unwrap_or_else(|| panic!("{EXAMPLE_FILE} is not among {CONFIG_FILES:?}"));

        let read = config::Config::builder()
            .add_source(config::File::from_str(EXAMPLE_SNIPPET, format))
            .build()
            .and_then(config::Config::try_deserialize::<Config>);

        assert!(
            read.as_ref()
                .is_ok_and(|cfg| !cfg.key.expose_secret().is_empty()),
            "{EXAMPLE_SNIPPET} in a {EXAMPLE_FILE} gives nostui no key: {read:?}"
        );
    }
}
