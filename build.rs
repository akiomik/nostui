fn main() {
    let git_output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .ok();
    let git_dir = git_output.as_ref().and_then(|output| {
        std::str::from_utf8(&output.stdout)
            .ok()
            .and_then(|s| s.strip_suffix('\n').or_else(|| s.strip_suffix("\r\n")))
    });

    // Tell cargo to rebuild if the head or any relevant refs change.
    if let Some(git_dir) = git_dir {
        let git_path = std::path::Path::new(git_dir);
        let refs_path = git_path.join("refs");
        if git_path.join("HEAD").exists() {
            println!("cargo:rerun-if-changed={}/HEAD", git_dir);
        }
        if git_path.join("packed-refs").exists() {
            println!("cargo:rerun-if-changed={}/packed-refs", git_dir);
        }
        if refs_path.join("heads").exists() {
            println!("cargo:rerun-if-changed={}/refs/heads", git_dir);
        }
        if refs_path.join("tags").exists() {
            println!("cargo:rerun-if-changed={}/refs/tags", git_dir);
        }
    }

    let git_output = std::process::Command::new("git")
        .args(["describe", "--always", "--tags", "--long", "--dirty"])
        .output()
        .ok();
    let git_info = git_output
        .as_ref()
        .and_then(|output| std::str::from_utf8(&output.stdout).ok().map(str::trim));

    if let Some(git_info) = git_info {
        println!("cargo:rustc-env=_GIT_INFO={}", git_info);
    }
}
