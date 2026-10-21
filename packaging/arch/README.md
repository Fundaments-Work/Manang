# Arch Linux package

`PKGBUILD` installs the prebuilt Linux x86_64 release of Manang (a public beta).
Nothing is compiled. It downloads the release archive from GitHub and checks its
SHA-256.

```bash
git clone https://github.com/Fundaments-Work/Manang.git
cd Manang/packaging/arch
makepkg -si
```

To publish it to the AUR as `manang-bin` once an account exists: copy `PKGBUILD`
and `.SRCINFO` into the AUR git repository for `manang-bin`. On each new release,
change `_tag`, `pkgver` and the checksums together, run `makepkg -g` for the new
sums, then regenerate `.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO`.

