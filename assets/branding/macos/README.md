# macOS icon overrides

Two kinds of icon can go here; both are optional.

- **Modern icon (macOS 26 Liquid Glass and later):** an Icon Composer file
  (`AppIcon.icon`, any name ending in `.icon`), or an Xcode asset catalog
  (`Assets.xcassets`) containing one app icon set. Saved straight from Icon
  Composer or copied out of an Xcode project; no changes needed. The macOS
  build (`packaging/macos/build.sh`) compiles it into the app with Xcode's
  `actool`, so building with it needs Xcode (not only the Command Line Tools).
- **Classic icon:** one `.icns` file (any name except `document.icns`),
  used instead of the one made from `logo` in `branding.toml`. It is what
  older macOS versions show, and what is used when the modern icon can't be
  compiled.
- **`.badge` file icon:** `document.icns` replaces the one made from
  `document` in `branding.toml`. Finder shows it for `.badge` files.

Keep at most one `.icon`, one `.xcassets` and one app `.icns`. With both an
`.icon` and an `.xcassets`, the `.icon` is used.
