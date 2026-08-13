# Changelog

All notable changes to `ochub-ui` are documented here.

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
