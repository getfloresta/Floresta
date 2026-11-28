// SPDX-License-Identifier: MIT OR Apache-2.0

fn main() {
    let version = get_version_from_git().unwrap_or_else(|| get_version_from_manifest().unwrap());
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let runtime = std::env::var("CARGO_CFG_TARGET_ENV").unwrap();

    let rustc = std::process::Command::new("rustc")
        .args(["--version"])
        .output()
        .map(|output| {
            assert!(
                output.status.success(),
                "Failed to run rustc --version. Is rustc installed?"
            );
            String::from_utf8(output.stdout).expect("Failed to parse rustc version")
        })
        .expect("Failed to run rustc --version. Is rustc installed?");

    let long_version = format!("version {version} compiled for {arch}-{os}-{runtime} with {rustc}");
    let user_agent = format!("/Floresta:{}/", version.strip_prefix('v').unwrap_or(&version));

    println!("cargo:rustc-env=LONG_VERSION={long_version}");
    println!("cargo:rustc-env=GIT_DESCRIBE={version}");
    println!("cargo:rustc-env=USER_AGENT={user_agent}");

    // re-run if either the build script or the git HEAD changes
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=build.rs");
}

fn get_version_from_manifest() -> Result<String, std::io::Error> {
    let manifest = std::fs::read_to_string("Cargo.toml")?;
    let toml: toml::Value = toml::from_str(&manifest).unwrap();
    Ok(format!("v{}", toml["package"]["version"].as_str().unwrap()))
}

fn get_version_from_git() -> Option<String> {
    std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .map(|output| {
            if !output.status.success() {
                return None;
            }
            let mut git_description = String::from_utf8(output.stdout).unwrap();
            git_description.pop(); // remove the trailing newline

            // If we don't pull tags, git will return the short commit id, which breaks the functional tests
            if !git_description.starts_with("v") {
                return None;
            }

            Some(git_description)
        })
        .ok()?
}
