use std::borrow::Cow;

use anyhow::Result;
use gpui_platform::application;
use ochub_ui::gpui::{
    App, AppContext, AssetSource, Bounds, SharedString, WindowBounds, WindowOptions, px, size,
};
use ochub_ui::{assets, gallery::GalleryView, theme};

struct GalleryAssets;

impl AssetSource for GalleryAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(assets::load(path))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(assets::list(path))
    }
}

fn main() {
    application()
        .with_assets(GalleryAssets)
        .run(|cx: &mut App| {
            ochub_ui::install(cx);
            theme::install_family(
                &theme::ochub_family(),
                theme::ThemeMode::System,
                cx.window_appearance(),
            );

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1000.), px(820.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(760.), px(600.))),
                    window_background: theme::window_background_appearance(),
                    ..Default::default()
                },
                |_window, cx| cx.new(GalleryView::new),
            )
            .expect("open ochub-ui component gallery");
        });
}
