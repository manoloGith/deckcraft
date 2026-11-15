//! Dark paper: in a dark theme, the editor can draw slides on light paper in dark grey with light
//! ink instead of glaring white ([`deckcraft_render::DarkPaper`]).
//!
//! Off unless the host turns it on ([`set`]), which a host does when its theme turns dark. Only
//! the editor's slide, thumbnails and layout previews change: exports, printing and the slide
//! show keep the presentation's colours, and the presentation itself is never changed.

use std::sync::{PoisonError, RwLock};

use deckcraft_color::Rgba;
use deckcraft_render::DarkPaper;

static DARK_PAPER: RwLock<Option<DarkPaper>> = RwLock::new(None);

/// Turn dark paper on with `paper` and `ink` (sRGB), or off with `None`.
pub fn set(colors: Option<([u8; 3], [u8; 3])>) {
    let dp = colors.map(|([pr, pg, pb], [ir, ig, ib])| DarkPaper { paper: Rgba::rgb(pr, pg, pb), ink: Rgba::rgb(ir, ig, ib) });
    *DARK_PAPER.write().unwrap_or_else(PoisonError::into_inner) = dp;
}

/// The paper while it is on.
pub fn get() -> Option<DarkPaper> {
    *DARK_PAPER.read().unwrap_or_else(PoisonError::into_inner)
}

/// A value that changes when dark paper is turned on, off or to other colours, for texture keys.
pub(crate) fn key() -> u64 {
    get().map_or(0, |dp| {
        let c = |c: Rgba| (u64::from(c.r) << 16) | (u64::from(c.g) << 8) | u64::from(c.b);
        (1 << 62) | (c(dp.paper) << 24) | c(dp.ink)
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use deckcraft_render::{RenderOpts, render_slide};

    use super::*;

    const DP: DarkPaper = DarkPaper { paper: Rgba::rgb(0x1E, 0x1E, 0x1E), ink: Rgba::rgb(0xD6, 0xD6, 0xD6) };

    fn sample() -> deckcraft_model::Presentation {
        let mut s = deckcraft_engine::Session::new();
        deckcraft_engine::sample::open_sample(&mut s).unwrap();
        (*s.active().unwrap().doc).clone()
    }

    fn pixel(img: &deckcraft_render::Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [img.pixels[i], img.pixels[i + 1], img.pixels[i + 2], img.pixels[i + 3]]
    }

    fn opts(dark: Option<DarkPaper>) -> RenderOpts<'static> {
        RenderOpts { scale: 0.25, dark_paper: dark, ..Default::default() }
    }

    /// The sample's title slide is on white: with dark paper its paper is dark grey; its dark
    /// "The Science" slide keeps its colours; without dark paper nothing changes.
    #[test]
    fn light_slides_go_dark_and_coloured_ones_stay() {
        let doc = sample();
        let (light, dark) = (0, 2);
        let plain = render_slide(&doc, light, &opts(None));
        assert_eq!(&pixel(&plain, 4, 4)[..3], &[255, 255, 255], "the title slide is on white");
        let on_paper = render_slide(&doc, light, &opts(Some(DP)));
        assert_eq!(&pixel(&on_paper, 4, 4)[..3], &[0x1E, 0x1E, 0x1E], "white paper turns dark grey");
        let navy = render_slide(&doc, dark, &opts(None));
        assert_eq!(render_slide(&doc, dark, &opts(Some(DP))).pixels, navy.pixels, "a dark slide keeps its colours");
    }
}
