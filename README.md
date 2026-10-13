# ochub-ui

`ochub-ui` is an independently versioned GPUI component library used by OcHub and OcHerdr.
It contains reusable theme primitives, layouts, controls, text and code
editors, charts, notifications, window chrome, and embedded assets.

The crate deliberately does not define OcHub product screens, navigation,
business view models, storage, or networking. Consumers compose the generic
components into their own application UI.

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
ochub-ui = { version = "0.5.1", git = "https://github.com/OcHub-team/ochub-ui", tag = "v0.5.1" }
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

## Shared input and shortcuts

The coordinated 0.8.0 development snapshot uses Rust 1.98.1 and the GPUI fork
listed in `COMPATIBILITY.md`. Applications must pin the same tested Git commit;
this snapshot does not imply that a release tag has been published.

`text_input::init` is idempotent. `TextInput` owns native editing, selection,
IME protection, undo, Find, and macOS plain-text paste and transpose. Hosts that
route raw keys into terminals can call `text_input::dispatch_focused_editing_key`
first; it only acts on the focused field and retains weak input references.

`shortcuts::Shortcut` supplies one platform binding for action registration,
raw matching, and help labels. Application commands stay in the host.
`KeyReleaseTracker` pairs consumed key-downs with the correct releases;
`ShortcutReveal` respects reduced motion when revealing reserved-space hints.
`help_row` presents readable, accessible shortcut references.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```

Licensed under MIT OR Apache-2.0. See [`LICENSE-MIT`](LICENSE-MIT),
[`LICENSE-APACHE`](LICENSE-APACHE), and [`NOTICE`](NOTICE). `NOTICE` also
records the GPUI pin's possible transitive GPL risk; this crate's own source
is permissive, but binaries that link GPUI must still be audited.
