fn main() {
    embed_comctl32_v6_in_test_binaries();
    tauri_build::build();
}

/// Embed ComCtl32 v6 so `cargo test` harnesses load on Windows.
///
/// Tauri embeds this manifest into the shipped app binary only (`rustc-link-arg-bins`).
/// Unit-test executables bind legacy comctl32 v5 and crash with
/// STATUS_ENTRYPOINT_NOT_FOUND before any test runs (tauri-apps/tauri#13419).
fn embed_comctl32_v6_in_test_binaries() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
