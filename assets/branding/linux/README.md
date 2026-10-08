# Linux icon overrides

Files here replace icons made from the artwork named in `branding.toml`, on
Linux only. Sizes you don't provide are still made from that artwork.

**App icon** (this folder):

- `<N>.png` for any of the sizes 16, 22, 24, 32, 48, 64, 96, 128, 256 and 512
  (exactly N x N pixels), e.g. `16.png`, `512.png`
- `scalable.svg` for the scalable icon

**`.badge` file icon** (the `document/` folder): the same names, e.g.
`document/48.png` and `document/scalable.svg`.

Apply with `./build.sh`, or
`cargo run --release --manifest-path packaging/branding/Cargo.toml`.
