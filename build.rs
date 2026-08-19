use std::process::Command;

fn main() {
    let hash = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()));

    if let Some(hash) = hash {
        println!("cargo:rustc-env=GIT_HASH={hash}");
        println!("cargo:rustc-env=BUILD_PROVENANCE_KIND=git");
        println!("cargo:rustc-env=BUILD_PROVENANCE_NOTE=git commit embedded at build time");
    } else {
        println!("cargo:rustc-env=BUILD_PROVENANCE_KIND=tarball");
        println!("cargo:rustc-env=BUILD_PROVENANCE_NOTE=no .git in source archive");
    }

    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git");
}
