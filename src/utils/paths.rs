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

/// A directory nostui uses, as a place rather than as a name. A relative one means a
/// different directory from every shell, and any of the sources below can give one.
///
/// Only a relative one is touched. An absolute path already names one place, so there is
/// nothing left to work out, and `absolute` does not leave a path alone: it drops `.`
/// components anywhere and collapses `..` on Windows. Rewriting an answer is only a way
/// to get it wrong.
///
/// Absolute, not canonical: `canonicalize` asks the filesystem and fails on the directory
/// that is not there, which is the case this exists to name.
///
/// A process whose working directory has gone gets the relative name back, and there is
/// no better answer: `absolute` is the thing that asks where the process is, and nothing
/// else here knows.
fn resolved(directory: PathBuf) -> PathBuf {
    if directory.is_absolute() {
        return directory;
    }

    // The one input `absolute` refuses, and where names are joined onto nothing.
    let directory = if directory.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        directory
    };

    path::absolute(&directory).unwrap_or(directory)
}

pub fn get_data_dir() -> PathBuf {
    resolved(if let Some(s) = DATA_FOLDER.clone() {
        s
    } else if let Some(proj_dirs) = project_directory() {
        proj_dirs.data_local_dir().to_path_buf()
    } else {
        PathBuf::from(".").join(".data")
    })
}

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

    /// `tests/first_run.rs` reaches this through the binary, but only on Unix — see the
    /// case there for why.
    #[test]
    fn an_empty_directory_resolves_to_the_working_one() -> Result<(), io::Error> {
        assert_eq!(resolved(PathBuf::new()), env::current_dir()?);

        Ok(())
    }

    /// Not merely already resolved — untouched, which `absolute` would not leave it: it
    /// drops a `.` on any platform and collapses a `..` on Windows.
    ///
    /// Spelled per platform because `is_absolute` is: `\srv` has a root and no prefix on
    /// Windows, which does not count, and the early return this is about would be skipped.
    #[test]
    fn an_absolute_directory_is_left_as_it_is() {
        let root = if cfg!(windows) { r"C:\srv" } else { "/srv" };
        let directory = PathBuf::from(root).join(".").join("cfg");

        // Compared as text: `Path`'s own equality reads components, and `.` is not one of
        // them — `/srv/./cfg` and `/srv/cfg` are equal to it, which is the difference
        // this is here to see.
        assert_eq!(
            resolved(directory.clone()).as_os_str(),
            directory.as_os_str()
        );
    }

    #[test]
    fn a_relative_directory_resolves_against_the_working_one() -> Result<(), io::Error> {
        assert_eq!(
            resolved(PathBuf::from("cfg")),
            env::current_dir()?.join("cfg")
        );

        Ok(())
    }
}
