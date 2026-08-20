# Compatibility policy

GPUI does not currently provide a stable component ABI for the APIs used here.
Every `ochub-ui` release therefore identifies one exact Zed Git revision.

| ochub-ui | Zed / GPUI revision | Minimum Rust |
|---|---|---|
| 0.4.x | `a8fafdd7ee36fb3fb98ebbfe5d3be983301d9e74` | 1.97.0 |
| 0.3.x | `a8fafdd7ee36fb3fb98ebbfe5d3be983301d9e74` | 1.97.0 |
| 0.2.x | `a8fafdd7ee36fb3fb98ebbfe5d3be983301d9e74` | 1.97.0 |
| 0.1.x | `a736da2a80964cf1b2f605bd7e8805f8da1acc52` | 1.97.0 |

Changing the Zed revision requires at least a minor `0.x` release and must pass
the Linux, macOS, and Windows CI matrix. Consumers should use the GPUI and
`zed-text` re-exports where practical so mismatched revisions cannot create
duplicate, incompatible Rust types.
