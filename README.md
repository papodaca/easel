# easel

easel is a desktop tool for marking up an image with labeled boxes and sending that markup to hosted image models. FLUX 3 is the first model. The window is gpui from the Zed tag `v1.24.2`, commit `fb69067729cb16a7f6135e2c1600c26a28c59659`.

Rust 1.98.1 comes from `rust-toolchain.toml`. Linux build and runtime packages are in [docs/dev-setup.md](docs/dev-setup.md).

```sh
cargo run -p easel
WAYLAND_DISPLAY='' cargo run -p easel
cargo nextest run -p easel-core
```
