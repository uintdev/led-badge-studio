# Contributing to LED Badge Studio

Bug reports, fixes and improvements are welcome. This page says how to get
a change accepted. The same rules apply whether you wrote the change
yourself or with the help of an AI agent.

## Open an issue first for scope and breaking changes

Before writing code for any of these, open an issue describing what you want
to change and why, and wait until it has been reviewed and agreed on:

- **New features**, or changes to what the app or `badgectl` is for.
- **Breaking changes**: anything that changes how existing users' files,
  scripts or installs behave. For example:
  - the `.badge` project format, or how damaged files are repaired;
  - `badgectl`'s commands, options, output or exit statuses;
  - the settings file, or where per-user files are kept;
  - the app ID, file type, program names or other package identifiers;
  - what is sent to the badge (the protocol, upload pacing);
  - the minimum Rust version, or the platforms supported.
- **Large refactors** or new dependencies.

Pull requests for these without an agreed issue may be closed, however good
the code is. Small, self-contained fixes (a bug, a typo, a clearer message)
don't need an issue first; say in the pull request what it fixes.

## Before you open a pull request

Run these from the project folder, and make sure they pass:

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

If `cargo fmt --all --check` lists differences, run `cargo fmt --all` to fix
them. The formatting settings are in `rustfmt.toml`. If you changed the
branding tool (`packaging/branding/`), run the same commands there too: add
`--manifest-path packaging/branding/Cargo.toml` (in the clippy command, before
the `--`).

How to set up a build is in [docs/building.md](docs/building.md); the code
layout is in [docs/development.md](docs/development.md).

## What a good pull request looks like

- **One change per pull request**, with a description of what it changes and
  why, and a link to its issue if it needed one.
- **Tests** for behavior you add or fix, where it can be tested: the window's
  behavior has tests in `crates/badge-app/src/app/window_tests.rs`, and each
  library crate has its own.
- **Hardware**: if the change affects what is sent to the badge or how it is
  found (`badge-core`'s protocol, `badge-io`), say which badge you tested it
  on (`badgectl list` shows it), or that you couldn't.
- **Docs**: update `README.md` or the files in `docs/` when you change
  something they describe.
- **Your own understanding**: you should be able to explain every line you
  submit, including any written with an AI tool.

## Conventions

- **Text in the app and the docs**: US English, plain hyphens (not em or en
  dashes), and short, plain tooltips (one sentence; details belong in the
  docs).
- **Icons**, not font symbols, in the interface: a test fails on symbol
  characters. Icons are SVG files in `crates/badge-app/assets/icons`.
- **Comments** explain why, not what: hardware quirks, protocol details,
  workarounds and safety notes. Don't narrate the code or its history.
- **How LEDs look** is described once, in
  `crates/badge-render/src/led_look.rs`; see
  [docs/development.md](docs/development.md) before changing it.

## Using AI

AI agents must strictly follow [AGENTS.md](AGENTS.md). The project prefers them for code review, bug fixes
and automated tests (see the [stance in the README](README.md#stance-on-ai)),
but AI-written changes are accepted on the same terms as any other.
