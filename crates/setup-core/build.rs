//! Puts the git commit into the build (`SETUP_ASSISTANT_COMMIT`), for recording sidecars.
//! Builds without git (or outside a checkout) simply have no commit.

use std::path::Path;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !s.is_empty()).then_some(s)
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Rerun when the checked-out commit changes: HEAD, the branch it points to, packed refs.
    // Only files that exist: a missing one would make cargo rerun this on every build.
    let branch = git(&["symbolic-ref", "-q", "HEAD"]);
    let watched = ["HEAD", "packed-refs"].into_iter().map(str::to_string).chain(branch);
    for name in watched {
        if let Some(p) = git(&["rev-parse", "--git-path", &name]).filter(|p| Path::new(p).exists()) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    if let Some(commit) = git(&["rev-parse", "--short=12", "HEAD"]) {
        println!("cargo:rustc-env=SETUP_ASSISTANT_COMMIT={commit}");
    }
}
