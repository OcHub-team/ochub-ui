//! Assets embedded into the library binary.
//!
//! GPUI applications have one top-level [`gpui::AssetSource`]. Host asset
//! sources can delegate to [`load`] and [`list`] before consulting their own
//! on-disk resources, which keeps component icons available without copying
//! files into every application bundle.

use std::borrow::Cow;
use std::sync::OnceLock;

use gpui::SharedString;

const CJK_FONT_PATH: &str = "fonts/OcHubNotoSansCJK-Regular.otf";
static MONOSPACE_FONT: OnceLock<String> = OnceLock::new();
const COMMON_MONOSPACE_FAMILIES: &[&str] = &[
    "Fira Code",
    "Fira Mono",
    "Source Code Pro",
    "Hack",
    "Iosevka",
    "Inconsolata",
    "JetBrains Mono",
    "IBM Plex Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
    "Ubuntu Mono",
    "Ubuntu Sans Mono",
    "Droid Sans Mono",
    "Roboto Mono",
    "Cascadia Mono",
    "Cascadia Code",
    "Consolas",
    "Menlo",
    "Monaco",
    "SF Mono",
    "Courier New",
];

/// An installed monospace family, selected after the host registers its fonts.
/// GPUI resolves concrete family names; the CSS name `monospace` and macOS's
/// Menlo are not portable substitutes on Linux or Windows.
pub fn monospace_font_family() -> &'static str {
    MONOSPACE_FONT
        .get()
        .map(String::as_str)
        .unwrap_or(".SystemUIFont")
}

fn choose_monospace_font(available: &[String], preferred: &[&str]) -> String {
    // GPUI includes virtual names such as `.ZedMono` in all_font_names even
    // when their backing font is absent. Match concrete, known monospace
    // families rather than a substring: e.g. Monotype Corsiva is proportional.
    preferred
        .iter()
        .chain(COMMON_MONOSPACE_FAMILIES.iter())
        .find_map(|preferred| {
            available
                .iter()
                .find(|name| !name.starts_with('.') && name.eq_ignore_ascii_case(preferred))
        })
        .cloned()
        // A readable system UI face is the explicit last resort when none of
        // the supported monospace families is installed.
        .unwrap_or_else(|| ".SystemUIFont".to_string())
}

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

/// Register the compact CJK fallback used by browser builds.
///
/// Native platforms can normally find system CJK fonts, while GPUI Web starts
/// with a deliberately small Latin-only font set. Registering the same subset
/// everywhere keeps Chinese and Japanese component labels deterministic.
pub fn install_fonts(cx: &mut gpui::App) {
    let font = load(CJK_FONT_PATH).expect("embedded OcHub CJK font");
    cx.text_system()
        .add_fonts(vec![font])
        .expect("register embedded OcHub CJK font");

    let preferred: &[&str] = if cfg!(target_os = "macos") {
        &["Menlo", "Monaco", "SF Mono"]
    } else if cfg!(target_os = "windows") {
        &["Consolas", "Cascadia Mono", "Cascadia Code", "Courier New"]
    } else {
        &[
            "DejaVu Sans Mono",
            "Liberation Mono",
            "Noto Sans Mono",
            "Ubuntu Mono",
        ]
    };
    let _ = MONOSPACE_FONT.set(choose_monospace_font(
        &cx.text_system().all_font_names(),
        preferred,
    ));
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

    #[test]
    fn embeds_cjk_font_and_license() {
        assert!(load(CJK_FONT_PATH).is_some());
        assert!(load("fonts/LICENSE-NOTO-CJK.txt").is_some());
    }

    #[test]
    fn missing_platform_monospace_uses_an_installed_family() {
        let available = vec!["Noto Sans CJK SC".into(), "Liberation Mono".into()];
        assert_eq!(
            choose_monospace_font(&available, &["Menlo"]),
            "Liberation Mono"
        );
        assert_eq!(
            choose_monospace_font(&["Noto Sans CJK SC".into()], &["Menlo"]),
            ".SystemUIFont"
        );
    }

    #[test]
    fn virtual_alias_does_not_hide_an_installed_code_font() {
        let available = vec![
            ".ZedMono".into(),
            ".SystemUIFont".into(),
            "Fira Code".into(),
            "Noto Sans".into(),
        ];
        assert_eq!(choose_monospace_font(&available, &["Menlo"]), "Fira Code");
    }

    #[test]
    fn common_code_fonts_do_not_need_mono_in_their_name() {
        for family in ["Source Code Pro", "Hack", "Iosevka", "Inconsolata"] {
            let available = vec![".ZedMono".into(), family.into(), "Noto Sans".into()];
            assert_eq!(choose_monospace_font(&available, &["Menlo"]), family);
        }
    }

    #[test]
    fn only_virtual_aliases_or_unrecognized_faces_use_system_ui_fallback() {
        let aliases = vec![".ZedMono".into(), ".ZedSans".into(), ".SystemUIFont".into()];
        assert_eq!(
            choose_monospace_font(&aliases, &[".ZedMono", "Menlo"]),
            ".SystemUIFont"
        );
        assert_eq!(
            choose_monospace_font(&["Monotype Corsiva".into()], &["Menlo"]),
            ".SystemUIFont"
        );
    }
}
