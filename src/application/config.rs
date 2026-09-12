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

        let mut found_config = false;
        for (file, format) in &CONFIG_FILES {
            let path = config_dir.join(file);

            builder = builder.add_source(
                config::File::from(path.clone())
                    .format(*format)
                    .required(false),
            );
            if path.exists() {
                found_config = true;
            }
        }
        if !found_config {
            // One binding for both, so stderr and the log cannot come to call the same
            // failure different things — matching a pasted terminal against a pasted log
            // is the whole use of having it in both.
            let found_nothing = format!("No configuration file found in {}", config_dir.display());
            let message = format!(
                "{found_nothing}\n\
                 Write {EXAMPLE_FILE} in that directory — create it first if it is not \
                 there:\n\
                 \x20   {EXAMPLE_SNIPPET}\n\
                 An npub instead of an nsec starts nostui read-only."
            );

            // The first line only. A `tracing` event is prefixed with its level and
            // location once however many lines it spans, so a `grep` for ERROR would take
            // that line and leave the instructions behind it — and they are for the
            // person at the terminal, who has them there.
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
                // The pair `Config::new` accepts, not one of the two: the error tells
                // people to write `key`.
                //
                // Neither assertion here fails on its own. This one needs the guard above
                // gone *and* a configuration with no key; the relays one below needs the
                // bundled `.config/config.json5` stripped of its own. A run against a
                // directory this test owns is what would stop that being luck: #578.
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
