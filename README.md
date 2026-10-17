# easel

easel is a desktop tool for marking up an image with labeled boxes and sending that markup to hosted image models. The window is gpui from the Zed tag `v1.24.2`, commit `fb69067729cb16a7f6135e2c1600c26a28c59659`.

| Model | Boxes | Image inputs | Size | Auth |
| --- | --- | --- | --- | --- |
| FLUX 3 | yes, compiled into the prompt | up to 10, one `images` list | aspect ratio and resolution | `x-key` (`BFL_API_KEY`) |
| FLUX.2 [pro] | kept on the canvas, not sent | up to 8, `input_image` … `input_image_8` | width and height, multiples of 16, at most 4 MP | `x-key` (`BFL_API_KEY`) |
| Krea 3 | kept on the canvas, not sent | up to 8 `image_urls`, uploaded first | aspect ratio and `1K`/`2K` | Bearer (`KREA_API_KEY`) |
| Krea 2 Large | kept on the canvas, not sent | one `image_url` plus up to 10 style references | aspect ratio, `1K` only | Bearer (`KREA_API_KEY`) |

Krea 3 is beta-gated. A 401 is reported as missing beta access only when the same key is accepted by `GET /jobs/{id}`. Uploaded Krea references may persist on Krea's side. Style and moodboard ids are typed in; the app does not browse `GET /styles`.

The window draws its own decorations and ships IBM Plex Sans (SIL OFL, `crates/easel/assets/fonts`) so it looks the same on every desktop.

Rust 1.98.1 comes from `rust-toolchain.toml`. Linux build and runtime packages are in [docs/dev-setup.md](docs/dev-setup.md).

```sh
cargo run -p easel
WAYLAND_DISPLAY='' cargo run -p easel
cargo nextest run -p easel-core
```
