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

/// The one the error names when there is no configuration at all. It is `config.json`
/// because that is what the README tells people to write, not because the others could
/// not carry the same snippet — `config.json5` and `config.yaml` both read JSON, and
/// only `config.toml` and `config.ini` do not.
///
/// It has to be in [`CONFIG_FILES`], under one of the formats that reads the snippet
/// shown beside it rather than JSON in particular;
/// `the_error_names_a_file_that_reads_the_json_it_shows` is what holds it to both.
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
            // The snippet is JSON, so the name shown beside it has to read JSON: the same
            // `{"key": …}` in a config.toml is a parse error, and in a config.ini it is a
            // key nostui reports as missing, which reads as the user having got the key
            // wrong rather than the format. `EXAMPLE_FILE` is therefore spelled out
            // rather than taken from the list, which cannot say which entries parse JSON,
            // and `the_error_names_a_file_that_reads_the_json_it_shows` is what holds the
            // two together. The alternatives do come from `CONFIG_FILES`, so a format
            // added or dropped there cannot leave them listing one nobody reads.
            //
            // Which name wins if two exist is deliberately unsaid: that is `config`'s
            // layering, and a sentence about it here would be one more thing to get wrong.
            let alternatives = CONFIG_FILES
                .iter()
                .map(|(file, _)| *file)
                .filter(|file| *file != EXAMPLE_FILE)
                .collect::<Vec<_>>()
                .join(", ");
            // "Make that directory" because nothing creates it: `initialize_logging` calls
            // `create_dir_all` for the data directory, and the config directory is only
            // ever read. Telling someone to write a file into a path that is not there
            // hands them one more thing to work out.
            let message = format!(
                "No configuration file found in {config_dir_str}\n\
                 Make that directory if it is not there, and write {EXAMPLE_FILE} in it, \
                 holding your key: {{\"key\": \"nsec1...\"}}\n\
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

    /// The error's last sentence is plural — "… are read too, each in its own format" —
    /// so it needs at least two names left once `EXAMPLE_FILE` is filtered out of
    /// [`CONFIG_FILES`]. One would print "config.json5 are read too" and none would leave
    /// the sentence without a subject at all, both of them on every fresh install.
    ///
    /// Asserted rather than branched on: neither is reachable from a fixed-length const
    /// of five, so a branch would be dead code and a comment would be a claim nothing
    /// holds to.
    #[test]
    fn the_error_has_at_least_two_formats_left_to_call_alternatives() {
        let alternatives = CONFIG_FILES
            .iter()
            .filter(|(file, _)| *file != EXAMPLE_FILE)
            .count();

        assert!(
            alternatives >= 2,
            "the error says {alternatives} of them \"are read too\""
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
                .any(|(file, format)| *file == EXAMPLE_FILE
                    && matches!(
                        format,
                        config::FileFormat::Json
                            | config::FileFormat::Json5
                            | config::FileFormat::Yaml
                    )),
            "the error shows JSON beside {EXAMPLE_FILE}, which {CONFIG_FILES:?} does not read"
        );
    }
}
