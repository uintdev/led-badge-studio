# Windows icon overrides

- **App icon:** put one `.ico` file here (any name except `document.ico`) to
  use it instead of the one made from `logo` in `branding.toml`. It is
  embedded in `led-badge-studio.exe` and `badgectl.exe`.
- **`.badge` file icon:** `document.ico` replaces the one made from
  `document` in `branding.toml`. Explorer shows it for `.badge` files once
  the app is installed with `build.cmd --install`.

For the best result an `.ico` should contain 16, 20, 24, 32, 40, 48, 64, 96
and 256 px images. Apply with `build.cmd`, or
`cargo run --release --manifest-path packaging/branding/Cargo.toml`.
