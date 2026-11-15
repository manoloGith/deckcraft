//! Dark paper: a slide on light paper drawn in dark grey with light ink, for a host that shows
//! the editor in a dark theme ([`RenderOpts::dark_paper`]). Painting only: the presentation is
//! never changed, and exports, printing and the slide show leave it off.
//!
//! Only slides whose background is near-white paper change. On them, near-white neutral paints
//! (the background, white fills, table cells) become the paper and near-black neutral ones (text,
//! lines, borders) the ink. Colours keep their colours, pictures are never recoloured, and inside
//! a shape or table cell with a fill of its own that is not paper (a coloured box, a gradient, a
//! picture) nothing changes, so white text on a blue box stays white.

use std::cell::Cell;

use deckcraft_color::Rgba;
use deckcraft_model::resolve::Ctx;
use deckcraft_model::style::Fill;

/// The paper and ink colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DarkPaper {
    pub paper: Rgba,
    pub ink: Rgba,
}

thread_local! {
    /// The colours while a light slide is being drawn with dark paper on.
    static ACTIVE: Cell<Option<DarkPaper>> = const { Cell::new(None) };
    /// Shapes and cells being drawn that have a fill of their own that is not paper.
    static HELD: Cell<u32> = const { Cell::new(0) };
}

/// Relative brightness, 0 (black) to 1 (white).
fn luminance(c: Rgba) -> f64 {
    (0.2126 * f64::from(c.r) + 0.7152 * f64::from(c.g) + 0.0722 * f64::from(c.b)) / 255.0
}

/// A grey: its channels close to each other.
fn neutral(c: Rgba) -> bool {
    let max = c.r.max(c.g).max(c.b);
    let min = c.r.min(c.g).min(c.b);
    max - min <= 24
}

fn light(c: Rgba) -> bool {
    neutral(c) && luminance(c) > 0.88
}

fn dark(c: Rgba) -> bool {
    neutral(c) && luminance(c) < 0.12
}

impl DarkPaper {
    /// `c` as painted on this paper ([`map`]).
    pub fn map(self, c: Rgba) -> Rgba {
        map(self, c)
    }
}

/// `c` as painted with dark paper `dp`: near-white neutrals become the paper and near-black ones
/// the ink, with `c`'s own alpha.
fn map(dp: DarkPaper, c: Rgba) -> Rgba {
    if light(c) {
        dp.paper.with_alpha(c.a)
    } else if dark(c) {
        dp.ink.with_alpha(c.a)
    } else {
        c
    }
}

/// `c` as painted now ([`map`] while dark paper is drawing and no coloured fill holds it back).
pub(crate) fn paint(c: Rgba) -> Rgba {
    match ACTIVE.with(Cell::get) {
        Some(dp) if HELD.with(Cell::get) == 0 => map(dp, c),
        _ => c,
    }
}

/// Whether a fill reads as paper: none, or a near-white neutral solid colour. Anything else
/// (a colour, a gradient, a picture, a pattern) is the shape's own and keeps what is on it.
pub(crate) fn is_paper(rctx: &Ctx, fill: Option<&Fill>, ph: Option<Rgba>) -> bool {
    match fill {
        None | Some(Fill::None | Fill::Group | Fill::Background) => true,
        Some(Fill::Solid { color }) => light(rctx.color(color, ph)) || rctx.color(color, ph).a == 0,
        Some(_) => false,
    }
}

/// While alive, dark paper draws (when `dp` is set and the slide's background `bg` is paper).
/// Restores what was before on drop, so nested and failed renders leave nothing behind.
pub(crate) struct Drawing {
    before: (Option<DarkPaper>, u32),
}

impl Drawing {
    pub(crate) fn start(dp: Option<DarkPaper>, paper_background: bool) -> Drawing {
        let before = (ACTIVE.with(Cell::get), HELD.with(Cell::get));
        ACTIVE.with(|a| a.set(dp.filter(|_| paper_background)));
        HELD.with(|h| h.set(0));
        Drawing { before }
    }
}

impl Drop for Drawing {
    fn drop(&mut self) {
        ACTIVE.with(|a| a.set(self.before.0));
        HELD.with(|h| h.set(self.before.1));
    }
}

/// While alive, what is drawn keeps its colours (a shape or cell with a fill of its own).
pub(crate) struct Hold {
    held: bool,
}

impl Hold {
    pub(crate) fn when(hold: bool) -> Hold {
        if hold {
            HELD.with(|h| h.set(h.get().saturating_add(1)));
        }
        Hold { held: hold }
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        if self.held {
            HELD.with(|h| h.set(h.get().saturating_sub(1)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DP: DarkPaper = DarkPaper { paper: Rgba::rgb(0x1E, 0x1E, 0x1E), ink: Rgba::rgb(0xD6, 0xD6, 0xD6) };

    #[test]
    fn white_becomes_paper_and_black_ink_keeping_alpha() {
        assert_eq!(map(DP, Rgba::WHITE), DP.paper);
        assert_eq!(map(DP, Rgba::rgb(0xF2, 0xF2, 0xF2)), DP.paper);
        assert_eq!(map(DP, Rgba::BLACK), DP.ink);
        assert_eq!(map(DP, Rgba::BLACK.with_alpha(128)), DP.ink.with_alpha(128));
    }

    #[test]
    fn colours_and_mid_greys_stay() {
        for c in [Rgba::rgb(0x44, 0x72, 0xC4), Rgba::rgb(0xFF, 0xEB, 0x3B), Rgba::rgb(0x80, 0x80, 0x80), Rgba::rgb(0x20, 0x20, 0x60)] {
            assert_eq!(map(DP, c), c);
        }
    }

    #[test]
    fn painting_follows_the_drawing_and_holds() {
        assert_eq!(paint(Rgba::WHITE), Rgba::WHITE, "off by default");
        {
            let _d = Drawing::start(Some(DP), false);
            assert_eq!(paint(Rgba::WHITE), Rgba::WHITE, "a slide on a coloured background keeps its colours");
        }
        {
            let _d = Drawing::start(Some(DP), true);
            assert_eq!(paint(Rgba::WHITE), DP.paper);
            {
                let _h = Hold::when(true);
                assert_eq!(paint(Rgba::WHITE), Rgba::WHITE, "white text on a coloured box stays white");
            }
            assert_eq!(paint(Rgba::BLACK), DP.ink);
        }
        assert_eq!(paint(Rgba::WHITE), Rgba::WHITE, "the drawing ends with the render");
    }
}
