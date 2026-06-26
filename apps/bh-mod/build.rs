use std::path::Path;
use std::process::Command;

// Bake the luwen git commit into the binary so a bare `bh-mod` can report the
// exact source it was built from via `--version`. Falls back to "unknown" when
// built outside a git checkout (e.g. from a release source tarball).
fn main() {
    let sha = run(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BH_MOD_GIT_SHA={sha}");

    // Rebuild when the checked-out revision changes.
    if let Some(git_dir) = run(&["rev-parse", "--git-dir"]) {
        let head = Path::new(&git_dir).join("HEAD");
        if head.exists() {
            println!("cargo:rerun-if-changed={}", head.display());
        }
        // Follow the symbolic ref (e.g. refs/heads/main) when on a branch so a
        // new commit on that branch triggers a rebuild.
        if let Some(symref) = run(&["symbolic-ref", "-q", "HEAD"]) {
            let ref_path = Path::new(&git_dir).join(symref);
            if ref_path.exists() {
                println!("cargo:rerun-if-changed={}", ref_path.display());
            }
        }
    }
}

fn run(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}
