# Linux setup

easel is a gpui app. The first `cargo` run clones Zed at the revision in `Cargo.lock` (on the order of 400 MB) and then compiles it.

Install rustup, then the system packages. `rust-toolchain.toml` selects Rust 1.98.1, rustfmt, and clippy.

```sh
sudo pacman -S --needed \
  clang cmake pkgconf openssl fontconfig \
  libxkbcommon libxkbcommon-x11 wayland libxcb \
  vulkan-icd-loader
```

Also install a Vulkan ICD for the GPU (`vulkan-radeon`, `nvidia-utils`, or the Intel equivalent) and `xdg-desktop-portal` with a desktop backend (`xdg-desktop-portal-gnome` on GNOME, `xdg-desktop-portal-gtk` otherwise).

```sh
cargo run -p easel
```

That uses Wayland when `WAYLAND_DISPLAY` is set. Force X11 / XWayland with an empty Wayland socket and `DISPLAY` still set:

```sh
WAYLAND_DISPLAY='' cargo run -p easel
```

With both `WAYLAND_DISPLAY` and `DISPLAY` unset, the process exits with an error instead of sitting headless.

`ZED_DEVICE_ID` picks a GPU. `lspci -nn | grep VGA` prints the id in brackets (`10de:2484` means `ZED_DEVICE_ID=0x2484`). gpui tries Vulkan and then GL. `WGPU_BACKEND=gl` is the GL fallback when RADV misbehaves under Wayland. `WAYLAND_DISPLAY=''` is the other escape, through XWayland.

```sh
cargo nextest run -p easel-core
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

`easel-core` tests do not build gpui.
