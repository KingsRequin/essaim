# Recettes communes à mes projets : `just verifier`, `just installer` (détails dans CLAUDE.md).
set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]

# Tests, clippy (-D warnings) et scripts de l'interface
verifier:
    cargo test --release
    cargo clippy --all-targets -- -D warnings
    node scripts/verifier-ui.mjs

# Compile en release, remplace l'instance installée et la relance
installer:
    .\scripts\install.ps1
