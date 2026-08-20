# Changelog

All notable changes to `ochub-ui` are documented here.

## 0.5.1 - 2026-08-21

- Expose the existing `TextInput::search_field` constructor so component-library
  consumers can use the canonical search-field treatment.

## 0.5.0 - 2026-08-21

- Add a plain `transparent` window background effect alongside the existing
  opaque and blurred treatments, allowing hosts to control translucency and
  backdrop blur independently.
- Keep legacy theme files compatible: omitted window effects still default to
  the blurred treatment.
- No GPUI revision change.

## 0.4.0 - 2026-08-20

- Restore the crate's boundary as a reusable component library.
- Remove the product-specific `screens` API. OcHub application and
  documentation compositions now live in their respective consumers.
- Relicense first-party source as MIT OR Apache-2.0. GPUI pin remains
  `a8fafdd7`; see `NOTICE` for transitive dependency license risk.
- Git tag `v0.4.0` is the intended release id after the license commit.
- No GPUI revision change.

## 0.3.3 - 2026-08-13

- Keep theme-card identity text at a usable width in two-column and narrow
  layouts instead of allowing GPUI flex shrink to wrap it one glyph per line.

## 0.3.2 - 2026-08-13

- Expand the embedded Noto Sans CJK subset to cover all current OcHub desktop
  and documentation labels, including the newly shared navigation and page copy.

## 0.3.1 - 2026-08-13

- Share the canonical gateway station card, usage summary grid, settings group,
  theme card and mode block, plus the common product-page frame.
- Remove the last page-structure copies from the GPUI Web documentation scenes.

## 0.3.0 - 2026-08-13

- Add portable product-level screen composition for the OcHub application
  shell, 252px navigation sidebar, provider list cards, active-provider hero,
  provider editor, MCP cards, skill cards, and session cards.
- Make screen APIs accept resolved labels, view-model values, and semantic
  action elements so native hosts keep storage/network access while the
  documentation site can render the same GPUI tree with demo adapters.
- Centralize provider-editor breakpoints and collapsed preview composition so
  desktop and WebAssembly layouts cannot drift independently.

## 0.2.0 - 2026-08-13

- Keep the embedded GPUI application handle alive for the browser page lifetime,
  preventing the WebAssembly gallery from being released before its first frame.
- Move to Zed / GPUI `a8fafdd7` for its re-entrant-safe web resize and frame
  callbacks, which allow WebGPU surfaces to adopt their real browser dimensions.
- Embed an OFL-licensed, component-catalog CJK fallback so Chinese and Japanese
  labels render consistently in GPUI Web instead of missing-glyph boxes.

## 0.1.2 - 2026-08-13

- Split the gallery's native and WebAssembly entry points so final WASM linking
  does not declare `main` twice.
- Strengthened WebAssembly CI from type checking to a complete linked build.

## 0.1.1 - 2026-08-13

- Added WebAssembly support for the component gallery and browser-compatible
  primitives through GPUI's WebGPU platform.
- Kept the Zed text-buffer-backed code editor desktop-only so browser builds do
  not pull native process and filesystem utilities.
- Added an explicit platform capability boundary to the public API.

## 0.1.0 - 2026-08-13

- Extracted OcHub's GPUI theme, layout, controls, editors, charts, diff view,
  notifications, scrollbar, animations, icons, and cross-platform window chrome.
- Removed dependencies on OcHub business and storage crates.
- Added host-configured theme storage and locale synchronization APIs.
- Embedded all component SVG assets in the library binary.
- Added a standalone component gallery and cross-platform CI.
