# Install Manang

Download the installer for your system from the
[latest release](https://github.com/Fundaments-Work/Manang/releases/latest). Every release lists its
files and a `SHA256SUMS.txt` so you can check what you downloaded.

| System | File | Notes |
| :--- | :--- | :--- |
| Windows 10 / 11 (64-bit) | `Manang_<version>_x64-setup.exe` or `Manang_<version>_x64_en-US.msi` | The `.exe` installs for your user without admin rights. The `.msi` suits managed machines. |
| macOS, Apple silicon (M1 and later) | `Manang_<version>_aarch64.dmg` | |
| macOS, Intel | `Manang_<version>_x64.dmg` | |
| Linux, Debian / Ubuntu | `Manang_<version>_amd64.deb` | `sudo apt install ./Manang_*.deb` |
| Linux, Fedora / openSUSE | `Manang-<version>-1.x86_64.rpm` | `sudo dnf install ./Manang-*.rpm` |
| Linux, any | `Manang_<version>_amd64.AppImage` | `chmod +x` it, then run it. |

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
  for you.

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
