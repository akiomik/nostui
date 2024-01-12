mod keybindings;
mod styles;

use std::path::PathBuf;

use color_eyre::eyre::Result;
use config::ConfigError;
use serde::Deserialize;

use crate::utils;

const CONFIG: &str = include_str!("../.config/config.json5");

#[derive(Clone, Debug, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub _data_dir: PathBuf,
    #[serde(default)]
    pub _config_dir: PathBuf,
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
    pub privatekey: String,
    #[serde(default)]
    pub relays: Vec<String>,
}

impl Config {
    pub fn new() -> Result<Self, config::ConfigError> {
        let default_config: Config = json5::from_str(CONFIG).unwrap();
        let data_dir = utils::get_data_dir();
        let config_dir = utils::get_config_dir();
        let mut builder = config::Config::builder()
            .set_default("_data_dir", data_dir.to_str().unwrap())?
            .set_default("_config_dir", config_dir.to_str().unwrap())?;

        let config_files = [
            ("config.json5", config::FileFormat::Json5),
            ("config.json", config::FileFormat::Json),
            ("config.yaml", config::FileFormat::Yaml),
            ("config.toml", config::FileFormat::Toml),
            ("config.ini", config::FileFormat::Ini),
        ];
        let mut found_config = false;
        for (file, format) in &config_files {
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
            log::error!("No configuration file found");
            return Err(ConfigError::Message(String::from(
                "No configuration file found",
            )));
        }

        let mut cfg: Self = builder.build()?.try_deserialize()?;

        for (mode, default_bindings) in default_config.keybindings.iter() {
            let user_bindings = cfg.keybindings.entry(*mode).or_default();
            for (key, cmd) in default_bindings.iter() {
                user_bindings
                    .entry(key.clone())
                    .or_insert_with(|| cmd.clone());
            }
        }
        for (mode, default_styles) in default_config.styles.iter() {
            let user_styles = cfg.styles.entry(*mode).or_default();
            for (style_key, style) in default_styles.iter() {
                user_styles
                    .entry(style_key.clone())
                    .or_insert_with(|| *style);
            }
        }

        if cfg.privatekey.is_empty() {
            return Err(ConfigError::NotFound(String::from("privatekey")));
        }

        if cfg.relays.is_empty() {
            cfg.relays = default_config.relays.clone();
        }

        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_config() {
        assert_eq!(Config::new().is_err(), true);

        // let c = Config::new()?;
        // assert_eq!(
        //     c.keybindings
        //         .get(&Mode::Home)
        //         .unwrap()
        //         .get(&parse_key_sequence("<q>").unwrap_or_default())
        //         .unwrap(),
        //     &Action::Quit
        // );
        // Ok(())
    }
}
