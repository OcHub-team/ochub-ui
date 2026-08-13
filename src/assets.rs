//! Assets embedded into the library binary.
//!
//! GPUI applications have one top-level [`gpui::AssetSource`]. Host asset
//! sources can delegate to [`load`] and [`list`] before consulting their own
//! on-disk resources, which keeps component icons available without copying
//! files into every application bundle.

use std::borrow::Cow;

use gpui::SharedString;

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

/// Load one library asset by its GPUI path.
pub fn load(path: &str) -> Option<Cow<'static, [u8]>> {
    EMBEDDED_ASSETS
        .iter()
        .find_map(|(asset_path, bytes)| (*asset_path == path).then_some(Cow::Borrowed(*bytes)))
}

/// List direct children below an asset directory.
pub fn list(path: &str) -> Vec<SharedString> {
    let prefix = path.trim_matches('/');
    let prefix = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}/")
    };
    let mut names = EMBEDDED_ASSETS
        .iter()
        .filter_map(|(asset_path, _)| {
            let remainder = asset_path.strip_prefix(&prefix)?;
            let name = remainder.split('/').next()?;
            (!name.is_empty()).then(|| SharedString::from(name.to_string()))
        })
        .collect::<Vec<_>>();
    names.sort_unstable_by(|left, right| left.as_ref().cmp(right.as_ref()));
    names.dedup_by(|left, right| left.as_ref() == right.as_ref());
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_component_icons() {
        assert!(load("icons/check.svg").is_some());
        assert!(
            list("icons")
                .iter()
                .any(|name| name.as_ref() == "check.svg")
        );
        assert!(list("icons").iter().any(|name| name.as_ref() == "agents"));
    }
}
