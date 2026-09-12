use std::sync::LazyLock;
use std::{env, path, path::PathBuf};

use directories::ProjectDirs;

pub static PROJECT_NAME: LazyLock<String> =
    LazyLock::new(|| env!("CARGO_CRATE_NAME").to_uppercase());
pub static DATA_FOLDER: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    env::var(format!("{}_DATA", *PROJECT_NAME))
        .ok()
        .map(PathBuf::from)
});
pub static CONFIG_FOLDER: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    env::var(format!("{}_CONFIG", *PROJECT_NAME))
        .ok()
        .map(PathBuf::from)
});
pub static LOG_ENV: LazyLock<String> = LazyLock::new(|| format!("{}_LOGLEVEL", *PROJECT_NAME));
pub static LOG_FILE: LazyLock<String> = LazyLock::new(|| format!("{}.log", env!("CARGO_PKG_NAME")));

fn project_directory() -> Option<ProjectDirs> {
    ProjectDirs::from("io", "0m1", env!("CARGO_PKG_NAME"))
}

/// A directory nostui uses, as a place rather than as a name: the variables can hold
/// anything, including nothing at all, and the fallbacks below are relative — each of
/// which names a different directory from every shell, where [`version`] prints both for
/// pasting into a bug report.
///
/// As far as `absolute` manages, which is neither canonical nor certain: it asks for the
/// working directory, so a process whose own has gone gets the relative name back. On
/// Unix it joins and drops `.` components, leaving `..` where it is; Windows answers through
/// `GetFullPathNameW`, which collapses it. Canonicalising instead would ask the
/// filesystem, and fail on the directory that is not there — the case this describes.
fn resolved(directory: PathBuf) -> PathBuf {
    // An empty path is the one thing `absolute` refuses, and it is where names are joined
    // onto nothing — the working directory.
    let directory = if directory.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        directory
    };

    path::absolute(&directory).unwrap_or(directory)
}

/// The directory nostui writes its log and its data into.
pub fn get_data_dir() -> PathBuf {
    resolved(if let Some(s) = DATA_FOLDER.clone() {
        s
    } else if let Some(proj_dirs) = project_directory() {
        proj_dirs.data_local_dir().to_path_buf()
    } else {
        PathBuf::from(".").join(".data")
    })
}

/// The directory nostui reads its configuration from. The error printed when there is no
/// configuration names it, and has to name the same place `--version` does.
pub fn get_config_dir() -> PathBuf {
    resolved(if let Some(s) = CONFIG_FOLDER.clone() {
        s
    } else if let Some(proj_dirs) = project_directory() {
        proj_dirs.config_local_dir().to_path_buf()
    } else {
        PathBuf::from(".").join(".config")
    })
}

/// The package version, and the directories this run would read and write.
///
/// The directories are the whole reason clap's own `version` is overridden: they are
/// what turns a pasted `--version` into a diagnosable bug report. The version itself
/// says nothing about the commit it was built from, deliberately — a `git describe`
/// used to be appended here and named the version twice in every binary this project
/// ever shipped ([#561](https://github.com/akiomik/nostui/issues/561)).
pub fn version() -> String {
    let author = clap::crate_authors!();

    let version = format!("v{}", env!("CARGO_PKG_VERSION"));

    // let current_exe_path = PathBuf::from(clap::crate_name!()).display().to_string();
    let config_dir_path = get_config_dir().display().to_string();
    let data_dir_path = get_data_dir().display().to_string();

    format!(
        "\
{version}

Authors: {author}

Config directory: {config_dir_path}
Data directory: {data_dir_path}"
    )
}

#[cfg(test)]
mod tests {
    use std::io;

    use pretty_assertions::assert_eq;

    use super::*;

    /// The one input `path::absolute` refuses, and the one a variable set to nothing
    /// produces. Reached through the binary by `tests/first_run.rs`, but only on Unix —
    /// `CreateProcess` drops an empty entry from the environment block — so on Windows
    /// this is the only thing that covers it.
    #[test]
    fn an_empty_directory_resolves_to_the_working_one() -> Result<(), io::Error> {
        assert_eq!(resolved(PathBuf::new()), env::current_dir()?);

        Ok(())
    }

    /// A relative name means a different place from every shell, which is the whole
    /// reason for resolving at all.
    #[test]
    fn a_relative_directory_resolves_against_the_working_one() -> Result<(), io::Error> {
        assert_eq!(
            resolved(PathBuf::from("cfg")),
            env::current_dir()?.join("cfg")
        );

        Ok(())
    }
}
