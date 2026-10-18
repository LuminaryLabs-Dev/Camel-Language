# Camel Setup

Native graphical installation wizard for Windows, macOS, and Linux. The distributable includes the wizard executable, a platform/architecture manifest, and checksummed Camel binaries. The end user does not need Rust or Cargo.

See [installation instructions](../docs/INSTALLATION.md).

## Build

```powershell
cargo build --release --manifest-path runtime/Cargo.toml --locked
cargo build --release --manifest-path installer/Cargo.toml --locked
# Optional native IDE:
cargo build --release --manifest-path studio/Cargo.toml --locked
pwsh ./scripts/package-preview.ps1 -IncludeStudio
```

Archive names include OS and architecture. Use `-Target <Rust triple>` for explicitly targeted builds; build all selected crates for that target first. Windows MSVC, Linux GNU, and macOS targets support x86_64 or aarch64 packaging. Each archive must contain binaries compiled for its target. The distribution workflow builds native artifacts on Windows, Linux, and macOS; it does not publish releases or change repository visibility.

## Ownership and failure behavior

- `bundle.rs` validates platform, file paths, and SHA-256 checksums. These detect corruption; they are not publisher signatures.
- `install.rs` owns staging, executable validation, receipts, replacement, and recovery. It refuses unrelated nonempty directories and installation inside the extracted bundle. Close running Camel/Studio processes before upgrading on Windows.
- `path.rs` stages per-user PATH changes and restores prior values if applying them fails. Windows uses the user environment variable through .NET. Unix uses managed blocks in shell configuration files and a fish snippet. Dotfile symlinks and unrelated configuration are preserved.
- `ui.rs` owns one worker and one wizard page at a time. Installation starts only after Review. Closing is blocked while its worker commits files and PATH.

Installation and PATH changes span multiple filesystem/registry operations, not a cross-process transaction. Run one installer at a time. Ordinary failure attempts restore the previous installation and PATH; forced shutdown during replacement can leave a `.camel-backup-*` folder beside the destination for manual recovery. User libraries remain in their separate data directory.

Unattended installation: run the native setup executable with `--bundle <extracted-folder> --install-to <absolute-folder>`. PATH is enabled by default; `--no-path` skips it. Success returns a JSON installation receipt. Normal launches open the graphical wizard.

## Checks

```powershell
cargo fmt --manifest-path installer/Cargo.toml --check
cargo clippy --manifest-path installer/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path installer/Cargo.toml --locked
```

The optional `capture` feature enables a screenshot of the real native renderer through `CAMEL_SETUP_CAPTURE_TO`; ordinary release builds exclude capture code. Tests use temporary directories and do not change the developer's actual PATH or install Camel for them.
