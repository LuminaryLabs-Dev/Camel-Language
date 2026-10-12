# Camel-Language
The Official Camel Agentic Development Language

## Install Camel

The distributable includes a native graphical setup wizard for Windows, macOS, and Linux. Extract the matching OS/CPU setup ZIP, launch its installer, and choose **Add Camel commands to my PATH**. Then open a new terminal and run `camel --version`.

See [installation instructions](docs/INSTALLATION.md) and the [setup build guide](installer/README.md). Preview distributions remain private until public access is authorized.

## Repository packages

| Package | Purpose | Documentation |
| --- | --- | --- |
| `camel-library-manager` | Shared Rust package for catalog discovery, HTTPS source-library installation, updates, removal, and installed-source lookup. | [Package guide](packages/camel-library-manager/README.md) |

Browse [repository packages](packages/README.md) and the [Camel source-library catalog](libraries/README.md).

The manager is available as a Git dependency from this repository. It has not been published to crates.io. Repository visibility controls anonymous access; the installer expects publicly readable source repositories. The package guide covers usage, contributions, checks, and current preview limitations.
