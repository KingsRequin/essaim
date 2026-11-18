// Manifeste intégré à l'exécutable : contrôles Windows modernes (Common Controls 6) pour la
// fenêtre des réglages. La sensibilité à l'échelle des écrans est réglée au lancement
// (`SetProcessDpiAwarenessContext`).
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' \
             version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    println!("cargo:rerun-if-changed=build.rs");
}
