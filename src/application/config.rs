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

/// The one the error names when there is no configuration at all, and the one the README
/// tells people to write. It has to be in [`CONFIG_FILES`] and it has to read the JSON
/// the error shows beside it — `the_file_the_error_names_is_one_the_program_reads` holds
/// it to the first, and the second is why it is not simply the head of that list.
const EXAMPLE_FILE: &str = "config.json";

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
            // Where it looked and what to write there. The path is the part a user cannot
            // guess — it differs per platform, and the README can only list all three —
            // and this is the first thing a fresh install prints, with no window open to
            // read anything else in (#113).
            //
            // Every name comes from `config_files` rather than being spelled again, so a
            // format added or dropped there cannot leave this recommending a file nobody
            // reads. Which of them wins if two exist is deliberately not said: that is
            // `config`'s layering, and a sentence about it here would be one more thing
            // to get wrong.
            // The snippet is JSON, so the name shown beside it has to be one that reads
            // JSON. The rest are offered as alternatives rather than as equals: the same
            // `{"key": …}` in a config.toml is a parse error, and in a config.ini it is a
            // key nostui reports as missing — which reads as the user having got the key
            // wrong rather than the format.
            //
            let alternatives = CONFIG_FILES
                .iter()
                .map(|(file, _)| *file)
                .filter(|file| *file != EXAMPLE_FILE)
                .collect::<Vec<_>>()
                .join(", ");
            let message = format!(
                "No configuration file found in {config_dir_str}\n\
                 Create {EXAMPLE_FILE} there, holding your key: \
                 {{\"key\": \"nsec1...\"}}\n\
                 An npub instead of an nsec starts nostui read-only. \
                 {alternatives} are read too, each in its own format."
            );

            log::error!("{message}");
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
                assert!(
                    !cfg.privatekey.expose_secret().is_empty(),
                    "privatekey should not be empty if config loads"
                );
                assert!(!cfg.relays.is_empty(), "relays should not be empty");
            }
            Err(e) => {
                // If it fails, it should be for expected reasons (no config file or no privatekey)
                println!("Config failed as expected: {e:?}");
                let err_msg = format!("{e:?}");
                assert!(
                    err_msg.contains("No configuration file found")
                        || err_msg.contains("privatekey"),
                    "Error should be about missing config file or privatekey, got: {e:?}",
                );
            }
        }
    }

    /// The error tells a user to write one particular file. Nothing else checks that
    /// `Config::new` still reads it: the name is spelled once and only filtered out of
    /// the list, so dropping the JSON entry would leave the message recommending a file
    /// the program ignores, with every other test green.
    #[test]
    fn the_file_the_error_names_is_one_the_program_reads() {
        assert!(
            CONFIG_FILES.iter().any(|(file, _)| *file == EXAMPLE_FILE),
            "the error names {EXAMPLE_FILE}, which is not among {CONFIG_FILES:?}"
        );
    }
}
