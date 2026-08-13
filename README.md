# ochub-ui

`ochub-ui` is the independently versioned GPUI design system used by OcHub.
It contains theme tokens, layout primitives, controls, text and code editors,
charts, notifications, window chrome, embedded icons, and portable OcHub
screen composition. Business services remain in the consuming application;
desktop and WebAssembly hosts provide view models and semantic actions to the
same product-level view tree.

The crate intentionally pins GPUI and the desktop-only `zed-text` integration
to one Zed revision. Use the re-exported `ochub_ui::gpui` types in custom
elements, and `ochub_ui::zed_text` on desktop, or pin direct dependencies to
the same revision documented in [`COMPATIBILITY.md`](COMPATIBILITY.md).

The component gallery, theme, layout, controls, text input, icons, charts, and
notifications support GPUI's WebGPU-backed WASM platform. The code editor is
desktop-only because Zed's `text` dependency currently includes native process
and filesystem utilities.

## Installation

Releases use semantic versions and matching Git tags. Depend on the release tag
instead of an arbitrary commit:

```toml
ochub-ui = { version = "0.3.3", git = "https://github.com/OcHub-team/ochub-ui", tag = "v0.3.3" }
```

```rust,ignore
use ochub_ui::{components, gpui, theme};

fn initialize(cx: &mut gpui::App) {
    ochub_ui::install(cx);
    theme::set_themes_dir("/path/to/application/themes");
}
```

Host applications should delegate unknown asset requests to
`ochub_ui::assets::load` and merge `ochub_ui::assets::list` into their asset
source. The icons are embedded in the library and do not need to be copied into
an application bundle.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```

Licensed under GPL-3.0-or-later.
