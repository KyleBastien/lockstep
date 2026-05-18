// libgit2-sys 0.17 (transitive via git2 0.19) does not emit
// `cargo:rustc-link-lib=advapi32` on the windows-msvc target. Without
// advapi32 the link step fails on Windows with unresolved imports like
// `RegQueryValueExW`, `OpenProcessToken`, `CryptAcquireContextA`. Emit the
// directive explicitly so test binaries link cleanly until libgit2-sys is
// bumped to 0.18+.
//
// Linux/macOS builds ignore this — the `cfg(windows)` guard makes the
// directive a no-op everywhere else.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
