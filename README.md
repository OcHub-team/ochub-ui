# ochub-ui

`ochub-ui` is the independently versioned GPUI design system used by OcHub.
It contains theme tokens, layout primitives, controls, text and code editors,
charts, notifications, window chrome, and embedded icons. Product views and
business services remain in the consuming application.

The crate intentionally pins GPUI and `zed-text` to one Zed revision. Use the
re-exported `ochub_ui::gpui` and `ochub_ui::zed_text` types in custom elements,
or pin direct dependencies to the same revision documented in
[`COMPATIBILITY.md`](COMPATIBILITY.md).

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
