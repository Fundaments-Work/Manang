# Install Manang

Download the installer for your system from the
[latest release](https://github.com/Fundaments-Work/Manang/releases/latest). Every release lists its
files and a `SHA256SUMS.txt` so you can check what you downloaded.

## Linux: one command

```bash
curl -fsSL https://raw.githubusercontent.com/Fundaments-Work/Manang/main/scripts/install-linux.sh | bash
```

The script picks the right package for your distribution (`.deb`, `.rpm` or a tarball), verifies it against the release's `SHA256SUMS.txt`, and installs it. Useful options (put them after `bash -s --`):

| Option | What it does |
| :--- | :--- |
| `--user` | Install the tarball into `~/.local`, no root needed |
| `--version 0.1.0-beta.1` | Install one specific release (the tag with or without `v`) |
| `--version beta` | Install the newest pre-release |
| `--channel latest\|beta\|stable` | Which release to install when no version is given |
| `--beta` / `--stable` | Shorthand for `--channel beta` / `--channel stable` |
| `--list` | Print every published release, newest first, and stop |
| `--print-latest` | Print the tag the default would install, and stop |
| `--bundle deb\|rpm\|tar` | Force a package format |
| `--dry-run` | Show what would be downloaded and change nothing |

### Beta and stable releases

Manang is in beta, so the plain command installs the newest release, pre-releases
included: that is `v0.1.0-beta.1` today. The two channels are always available:

```bash
curl -fsSL .../install-linux.sh | bash -s -- --beta    # newest pre-release
curl -fsSL .../install-linux.sh | bash -s -- --stable  # newest marked stable
curl -fsSL .../install-linux.sh | bash -s -- --list    # what has been published
```

While there is no stable release, `--stable` says so and installs nothing rather
than quietly installing a beta. GitHub's API allows 60 anonymous requests an
hour; if the script reports the limit, wait or set `GITHUB_TOKEN` to a token with
no scopes.

Remove it with `curl -fsSL https://raw.githubusercontent.com/Fundaments-Work/Manang/main/scripts/uninstall-linux.sh | bash` (add `-s -- --purge` to delete settings and plugins too). Read the script first if you like: it is short, and it is `scripts/install-linux.sh` in this repository.

## All systems

| System | File | Notes |
| :--- | :--- | :--- |
| Windows 10 / 11 (64-bit) | `Manang_<version>_x64-setup.exe` or `Manang_<version>_x64_en-US.msi` | The `.exe` installs for your user without admin rights. The `.msi` suits managed machines. |
| macOS, Apple silicon (M1 and later) | `Manang_<version>_aarch64.dmg` | |
| macOS, Intel | `Manang_<version>_x64.dmg` | |
| Linux, Debian / Ubuntu | `Manang_<version>_amd64.deb` | `sudo apt install ./Manang_*.deb` |
| Linux, Fedora / openSUSE | `Manang-<version>-1.x86_64.rpm` | `sudo dnf install ./Manang-*.rpm` |
| Linux, any | `Manang_<version>_amd64.tar.xz` | Unpack it and run `usr/bin/manang`, or use the script with `--bundle tar --user`. |

### Why there is no AppImage

The first upload of `v0.1.0-beta.1` had one; this release does not. Tauri builds AppImages with
linuxdeploy, which follows the binary's shared libraries and copies the whole GTK and
WebKitGTK stack into the file: 120 MB, of which 91 MB was `libwebkit2gtk` and 32 MB
`libjavascriptcoregtk`, for a 46 MB `.deb`. That copy is a browser engine frozen at build
time, so it never receives the security updates your system's does, and it breaks against
newer GTK versions. Tauri has no switch to leave it out, so the tarball takes its place:
31 MB, the same WebKitGTK 4.1 requirement as the packages, and no root.

## Check the download

```bash
sha256sum -c --ignore-missing SHA256SUMS.txt        # Linux
shasum -a 256 -c --ignore-missing SHA256SUMS.txt    # macOS
Get-FileHash .\Manang_*_x64-setup.exe               # Windows PowerShell; compare with the file
```

## The installers are not signed yet

Manang is built in the open on GitHub Actions, but we have not bought code-signing
certificates yet, so your system warns you the first time. This is expected, and it is why
the checksums above matter.

- **Windows SmartScreen** shows "Windows protected your PC". Choose **More info**, then
  **Run anyway**.
- **macOS Gatekeeper** says the app "is damaged" or "cannot be opened". Move Manang to
  Applications, then run this once in Terminal and open it again:
  ```bash
  xattr -dr com.apple.quarantine /Applications/Manang.app
  ```
- **Linux** has no such check.

## What you need besides Manang

- **Typst and Markdown** work out of the box.
- **LaTeX** needs a TeX installation on your `PATH` (TeX Live, MiKTeX or MacTeX), or the
  optional built-in engine, which is a consented download offered on Linux x86-64 first.
  See [Fixing errors](fixing-errors.md) and [Known issues](../beta/known-issues.md).
- **Linux** needs WebKitGTK 4.1 (`libwebkit2gtk-4.1`), which the `.deb` and `.rpm` pull in
  for you. The tarball does not install it: on Debian and Ubuntu
  `sudo apt install libwebkit2gtk-4.1-0`, on Fedora `sudo dnf install webkit2gtk4.1`, on Arch
  `sudo pacman -S webkit2gtk-4.1`.

## Updating

There is no in-app updater yet. Download the new installer and run it over the old one;
your projects and settings live in your own folders and are kept. Releases are listed in the
[changelog](../../CHANGELOG.md).

## Uninstalling

Use your system's normal way: Apps & features on Windows, drag to Trash on macOS,
`sudo apt remove manang` or `sudo dnf remove manang` on Linux. Your projects are ordinary
folders and are never touched.

## Build from source

See [Building](../building.md).
