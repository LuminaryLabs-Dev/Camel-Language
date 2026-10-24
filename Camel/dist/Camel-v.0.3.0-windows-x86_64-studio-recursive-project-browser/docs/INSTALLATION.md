# Install Camel

Obtain the Camel setup ZIP built for your operating system and CPU architecture, and extract the **entire archive** before starting setup. Distribution sharing remains private until public access is authorized.

## Start the graphical wizard

| Platform | Start setup | Default installation folder |
| --- | --- | --- |
| Windows | Double-click `Camel-Setup.exe`. | `%LOCALAPPDATA%\Programs\Camel` |
| macOS | Open `Install-Camel.command`, or run `sh Install-Camel.command`. | `~/Library/Application Support/Camel` |
| Linux | Run `sh install.sh`; it opens the native setup window. | `${XDG_DATA_HOME:-~/.local/share}/camel` |

Choose the installation folder, keep **Add Camel commands to my PATH** selected, review the choices, and click **Install Camel**. This installs for the current account. The default folder and user PATH do not require administrator access. Studio is installed when included in the bundle.

Open a new terminal after setup and run:

```text
camel --version
```

If the terminal was launched from an application that already had an old environment, restart that application or sign out and back in. Setup cannot rewrite the environment of terminals that are already running.

The wizard updates Windows user PATH. On macOS/Linux it adds marked PATH blocks to `.profile`, `.bashrc`, existing `.bash_profile`/`.bash_login`, and zsh `.zprofile`/`.zshrc` (respecting `ZDOTDIR`), plus a fish snippet (respecting `XDG_CONFIG_HOME`). Repeated setup replaces the block rather than duplicating it. Custom shells or dotfiles that later overwrite PATH may need to include `<installation>/bin` themselves.

PATH makes `camel`, `camel-audio-runtime`, and optional `camel-studio` available by command name.

## Upgrades and portable use

Run setup from a newly extracted archive and choose the existing installation folder to upgrade. Close running Camel or Studio processes first. The wizard checks the distribution and stages the new files before replacement. It identifies installations by `INSTALLATION.json` and refuses to replace unrelated folders. Store personal scripts/output outside the program installation folder; program files are replaced on upgrade. Library data stays in its separate user data location.

Uncheck the PATH option to install without changing the account environment. The extracted `bin/camel` or `bin/camel.exe` also works directly without installation.

## Platform requirements

Each OS needs matching native binaries. The graphical wizard uses OpenGL and needs a desktop display; headless machines can use the unattended options in the [installer guide](../installer/README.md). Linux runtime builds use ALSA, and the GUI needs desktop/OpenGL libraries. macOS application signing/notarization and Windows code signing are not configured in this preview; follow the OS controls for software from a trusted source.

## Build a distribution

Build the runtime and installer in release mode, plus Studio if needed. Run `pwsh ./scripts/package-preview.ps1 [-IncludeStudio] [-Target <Rust triple>]`. It creates an OS/architecture-specific setup ZIP, preserves existing archives, and includes `INSTALLER_BUNDLE.json` and `BUNDLE_MANIFEST.json` checksums. The current 0.3.0 candidate and CI target Windows only; Linux and macOS packaging are outside this release scope. CI does not publish releases automatically.

Platform behavior follows [Windows user environment variables](https://learn.microsoft.com/en-us/dotnet/api/system.environment.setenvironmentvariable), [Bash startup files](https://www.gnu.org/software/bash/manual/html_node/Bash-Startup-Files), [zsh startup files](https://zsh.sourceforge.io/Doc/Release/Files.html), and [fish PATH configuration](https://fishshell.com/docs/current/tutorial.html).
