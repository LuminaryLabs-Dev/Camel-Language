# camel-library-manager

Shared Rust package for discovering and managing Camel source libraries from a GitHub catalog. It owns catalog validation, HTTPS downloads, installation, removal, and installed-source lookup. The Camel executable owns CLI commands; the runtime owns language parsing and import semantics. Studio can reuse the same API through the runtime's `libraries` re-export.

Version: **0.3.0**. License: **Apache-2.0**.

[Repository](https://github.com/LuminaryLabs-Dev/Camel-Language) · [Package source](https://github.com/LuminaryLabs-Dev/Camel-Language/tree/main/packages/camel-library-manager) · [Library catalog](https://github.com/LuminaryLabs-Dev/Camel-Language/blob/main/libraries/index.json)

## Use from Rust

The package is distributed from GitHub; it has not been published to crates.io. Repository visibility controls access to Git dependencies and package links. Published GitHub sources must be publicly readable; this client does not authenticate private raw-file downloads. Workspace-only catalog entries can use `localPath` to install from a checkout or bundled package source instead.

```toml
[dependencies]
camel-library-manager = { git = "https://github.com/LuminaryLabs-Dev/Camel-Language.git", branch = "main" }
```

For a sibling crate in this repository:

```toml
camel-library-manager = { path = "../packages/camel-library-manager", version = "0.3.0" }
```

```rust,no_run
use camel_library_manager::{catalog, install};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = catalog(None)?;
    let library = catalog.libraries.iter()
        .find(|library| library.name == "standard")
        .ok_or("standard library is absent from the catalog")?;
    let directory = install(library)?;
    println!("Installed at {}", directory.display());
    Ok(())
}
```

Other APIs: `catalog_local()`, `installed_root()`, `installed_library(name)`, `resolve_import(path)`, `remove(name)`, `validate(library)`, `create_lock(catalog, roots)`, `read_lock(path)`, `write_lock(path, lock)`, `verify_lock(lock)`, and `install_locked(lock)`. `catalog(Some(source))` accepts a JSON file path or HTTP(S) URL. `catalog_local()` reads the catalog beside an installed executable, then the checked-out repository catalog, then an embedded snapshot; it never uses the network. `catalog(None)` retains remote fallback for CLI discovery. Studio uses `catalog_local()` on open and accesses the remote catalog only on Refresh. Caravan is a workspace-only `localPath` entry and installs from `libraries/caravan` in a checkout or bundled payload after verifying its file digest. Remote `standard` and `audio` installation still require public access to their GitHub sources. The package includes no interpreter, microphone backend, GUI, or standalone executable.

## Camel CLI integration

The current Camel development runtime integrates this package. These commands apply to that source runtime, not the historical alpha executable. Runtime and Studio source publication is separate from this standalone package:

```text
camel library list
camel library search standard
camel library install standard
camel library update standard
camel library remove standard
camel library list --catalog libraries/index.json
camel library lock standard audio caravan --output camel.lock
camel library verify-lock camel.lock
camel library install-locked camel.lock
```

Help and removal work offline. Installed sources are imported with `using "@camel/standard/core.camel";`. Normal relative imports take precedence. Resolution performs no network calls and rejects paths outside the installed package.

Set `CAMEL_HOME` to choose a data directory. Libraries are stored beneath its `libraries/` directory. Defaults: `%LOCALAPPDATA%/Camel/libraries` on Windows and `~/.camel/libraries` elsewhere.

## Catalog and installation contract

The [catalog contribution guide](../../libraries/README.md) documents adding source libraries. Schema version 3 entries provide a name, description, semantic library version, immutable full commit SHA, GitHub repository, relative directory, explicit file allowlist, per-file SHA-256 checksums, and exact-version dependencies. Downloads use the declared revision and verify bytes before installation, with a 60-second request timeout. `camel library update <name>` reads the latest remote catalog by default; pass `--catalog` for a local snapshot.

All files download into a temporary directory before the existing installation changes. Replacement uses a backup directory and restores it if the final rename fails. A subsequent install recovers a backup left by an interrupted replacement. Repeated installation replaces the snapshot; repeated removal returns `false` when already absent. Writes to the same package must be serialized by callers. Replacement spans multiple renames, so it is not a cross-process transaction.

Catalog versions and commit revisions let source libraries advance independently of the runtime. `create_lock` resolves exact direct and transitive dependencies deterministically; `verify_lock` checks the package graph and pinned metadata offline; `install_locked` downloads in dependency order and checks every source digest. Lockfiles and checksums do not authenticate publishers. A failed multi-package install can leave earlier packages installed; rerunning is safe. Installing a library downloads source; importing trusted source into a workflow may grant it that workflow's normal capabilities.

## Build and checks

From the repository root, with Rust 1.88 or newer:

```powershell
cargo check --manifest-path packages/camel-library-manager/Cargo.toml --locked
cargo test --manifest-path packages/camel-library-manager/Cargo.toml --locked
cargo package --manifest-path packages/camel-library-manager/Cargo.toml --locked --allow-dirty --no-verify
```

The final command creates a local `.crate` archive and does not publish it.
