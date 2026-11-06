// What each `navigation.tab-style` looks like.
//
// A style turns one tab into a list of `Shape`s and draws nothing itself, so
// the geometry can be tested on its own. `paint` hands the list to sugarloaf.
//
// Polygons are triangulated as a fan, so every one here has to be convex. The
// powerline arrows get their notched left edge for free: tabs are painted
// right to left, and each tab's tip lands on top of the one after it.

use super::island::{island_rect, ISLAND_HEIGHT, TAB_GAP};
use rio_backend::config::colors::Colors;
use rio_backend::config::navigation::{
    clamp_tab_radius, Navigation, TabCloseButton, TabSizing, TabStyle,
};
use rio_backend::sugarloaf::Sugarloaf;

/// How far a powerline arrow's tip reaches into the next tab.
const POWERLINE_TIP: f32 = 12.0;
/// How far a slanted powerline tab leans over the next one.
const POWERLINE_SLANT: f32 = 10.0;
/// The gap the strip shows through between powerline tabs.
const POWERLINE_GAP: f32 = 2.0;
const POWERLINE_INSET_Y: f32 = 4.0;
/// Segments in each half of a rounded powerline cap.
const POWERLINE_CAP_STEPS: usize = 12;

const ACCENT_LINE_HEIGHT: f32 = 2.0;
/// How far an underline stops short of each end of its tab.
const UNDERLINE_INSET_X: f32 = 10.0;
const UNDERLINE_OFFSET_Y: f32 = 6.0;

/// Where a tab that joins the terminal starts.
const CONNECTED_TOP: f32 = 6.0;
const CURVE_MAX_FLARE: f32 = 8.0;
const CURVE_FLARE_STEPS: usize = 8;
const SEPARATOR_INSET_Y: f32 = 12.0;

/// How much a strip of its own stands out from the window background.
const STRIP_TINT: f32 = 0.08;
const POWERLINE_INACTIVE_TINT: f32 = 0.14;
const EDGE_TINT: f32 = 0.12;
const INACTIVE_CUSTOM_MUTE: f32 = 0.55;
/// How far apart text and its fill have to be in luminance to stay readable.
const MIN_CONTRAST: f32 = 0.35;

/// One piece of a tab, in logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    },
    Rounded {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        /// Top left, top right, bottom right, bottom left.
        radii: [f32; 4],
        color: [f32; 4],
    },
    /// Must be convex.
    Polygon {
        points: Vec<(f32, f32)>,
        color: [f32; 4],
    },
}

/// The tab settings of a window, from the config.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabAppearance {
    pub style: TabStyle,
    pub radius: f32,
    pub close_button: TabCloseButton,
    pub sizing: TabSizing,
    pub fill: Option<[f32; 4]>,
    pub active_fill: Option<[f32; 4]>,
    pub strip: Option<[f32; 4]>,
    pub accent: [f32; 4],
}

impl Default for TabAppearance {
    fn default() -> Self {
        Self::from_config(&Navigation::default(), &Colors::default())
    }
}

impl TabAppearance {
    pub fn from_config(navigation: &Navigation, colors: &Colors) -> Self {
        Self {
            style: navigation.tab_style,
            radius: clamp_tab_radius(navigation.tab_radius),
            close_button: navigation.tab_close_button,
            sizing: navigation.tab_sizing,
            fill: colors.tabs_fill,
            active_fill: colors.tabs_active_fill,
            strip: colors.tabs_strip,
            accent: colors.tabs_accent.unwrap_or(colors.cursor),
        }
    }
}

pub struct IslandFills {
    pub inactive: [f32; 4],
    pub active: [f32; 4],
    pub outline: Option<[f32; 4]>,
    pub close_hover: [f32; 4],
}

pub fn island_fills(bg: [f32; 4]) -> IslandFills {
    if luminance(bg) > 0.5 {
        IslandFills {
            inactive: [0.0, 0.0, 0.0, 0.05],
            active: [1.0, 1.0, 1.0, 0.92],
            outline: Some([0.0, 0.0, 0.0, 0.14]),
            close_hover: [0.0, 0.0, 0.0, 0.09],
        }
    } else {
        IslandFills {
            inactive: [1.0, 1.0, 1.0, 0.05],
            active: [1.0, 1.0, 1.0, 0.18],
            outline: None,
            close_hover: [1.0, 1.0, 1.0, 0.14],
        }
    }
}

#[inline]
fn luminance(color: [f32; 4]) -> f32 {
    0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2]
}

#[inline]
pub fn over(dst: [f32; 4], src: [f32; 4]) -> [f32; 4] {
    let a = src[3];
    [
        src[0] * a + dst[0] * (1.0 - a),
        src[1] * a + dst[1] * (1.0 - a),
        src[2] * a + dst[2] * (1.0 - a),
        dst[3],
    ]
}

/// `color` nudged away from itself, toward white on a dark one.
fn tinted(color: [f32; 4], amount: f32) -> [f32; 4] {
    let toward = if luminance(color) > 0.5 { 0.0 } else { 1.0 };
    over(color, [toward, toward, toward, amount])
}

#[inline]
fn opaque(color: [f32; 4]) -> [f32; 4] {
    [color[0], color[1], color[2], 1.0]
}

/// `text`, unless it would vanish against `fill`.
pub fn readable(text: [f32; 4], fill: [f32; 4]) -> [f32; 4] {
    if (luminance(text) - luminance(fill)).abs() >= MIN_CONTRAST {
        text
    } else if luminance(fill) > 0.5 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [1.0, 1.0, 1.0, 1.0]
    }
}

/// Colors resolved against the window background for one frame.
pub struct Palette {
    pub bg: [f32; 4],
    pub strip: Option<[f32; 4]>,
    pub inactive: Option<[f32; 4]>,
    pub active: Option<[f32; 4]>,
    pub outline: Option<[f32; 4]>,
    pub close_hover: [f32; 4],
    pub accent: [f32; 4],
}

impl Palette {
    pub fn new(appearance: &TabAppearance, bg: [f32; 4]) -> Self {
        let fills = island_fills(bg);
        let style = appearance.style;

        let strip = appearance.strip.or(style.has_own_strip().then(|| {
            // Keeps the window's own alpha, so a translucent window stays so.
            tinted(bg, STRIP_TINT)
        }));
        let (inactive, active) = match style {
            TabStyle::Islands => (Some(fills.inactive), Some(fills.active)),
            TabStyle::Underline => (None, None),
            TabStyle::TopAccent | TabStyle::Connected | TabStyle::BrowserCurve => {
                (None, Some(bg))
            }
            TabStyle::PowerlineAngled
            | TabStyle::PowerlineSlanted
            | TabStyle::PowerlineRound => (
                Some(opaque(tinted(bg, POWERLINE_INACTIVE_TINT))),
                Some(opaque(appearance.accent)),
            ),
        };

        Self {
            bg,
            strip,
            inactive: appearance.fill.or(inactive),
            active: appearance.active_fill.or(active),
            outline: fills.outline,
            close_hover: fills.close_hover,
            accent: appearance.accent,
        }
    }
}

/// What a style needs to know about a tab, beyond where it is.
#[derive(Clone, Copy, Default)]
pub struct TabVisual {
    pub active: bool,
    /// Lifted out of the strip by a drag.
    pub floating: bool,
    /// From the color picker or `navigation.color-automation`.
    pub custom: Option<[f32; 4]>,
    /// Whether a divider belongs at this tab's right edge.
    pub divider: bool,
}

/// The fill a tab's body gets, if it has one.
pub fn body_fill(palette: &Palette, tab: &TabVisual) -> Option<[f32; 4]> {
    match tab.custom {
        Some(mut custom) => {
            if tab.floating {
                custom[3] = 1.0;
            } else if !tab.active {
                custom[3] *= INACTIVE_CUSTOM_MUTE;
            }
            Some(custom)
        }
        None if tab.active => palette.active,
        None => palette.inactive,
    }
}

/// The color of a line marking a tab, if it has one. A custom color takes the
/// line over, so a style with no fill can still show it.
fn accent_line(palette: &Palette, tab: &TabVisual) -> Option<[f32; 4]> {
    match tab.custom {
        Some(mut custom) => {
            if !tab.active {
                custom[3] *= INACTIVE_CUSTOM_MUTE;
            }
            Some(custom)
        }
        None => tab.active.then_some(palette.accent),
    }
}

/// The shapes of one tab in the slot at `x`, `w` wide.
pub fn tab_shapes(
    style: TabStyle,
    x: f32,
    w: f32,
    radius: f32,
    palette: &Palette,
    tab: &TabVisual,
) -> Vec<Shape> {
    match style {
        TabStyle::Islands => islands(x, w, radius, palette, tab),
        TabStyle::Underline => underline(x, w, radius, palette, tab),
        TabStyle::TopAccent => top_accent(x, w, palette, tab),
        TabStyle::Connected => connected(x, w, radius, palette, tab),
        TabStyle::BrowserCurve => browser_curve(x, w, radius, palette, tab),
        TabStyle::PowerlineAngled => {
            powerline(x, w, palette, tab, |x, w, top, bottom| {
                vec![
                    (x, top),
                    (x, bottom),
                    (x + w, bottom),
                    (x + w + POWERLINE_TIP, (top + bottom) / 2.0),
                    (x + w, top),
                ]
            })
        }
        TabStyle::PowerlineSlanted => powerline_slanted(x, w, palette, tab),
        TabStyle::PowerlineRound => powerline(x, w, palette, tab, round_cap),
    }
}

#[inline]
fn fitted_radius(radius: f32, w: f32, h: f32) -> f32 {
    radius.min(w / 2.0).min(h / 2.0).max(0.0)
}

#[inline]
fn uniform(radius: f32) -> [f32; 4] {
    [radius; 4]
}

fn islands(
    x: f32,
    w: f32,
    radius: f32,
    palette: &Palette,
    tab: &TabVisual,
) -> Vec<Shape> {
    let (ix, iy, iw, ih, _) = island_rect(x, w);
    let radius = fitted_radius(radius, iw, ih);

    let fill = match (tab.floating, tab.custom) {
        // Nothing may show through a tab lifted over the others.
        (true, None) => palette
            .active
            .map(|active| over(opaque(palette.bg), active)),
        _ => body_fill(palette, tab),
    };
    let Some(fill) = fill else {
        return Vec::new();
    };

    match palette.outline {
        Some(ring) => {
            let fw = (iw - 2.0).max(0.0);
            let fh = (ih - 2.0).max(0.0);
            let inner = (radius - 1.0).clamp(0.0, fw.min(fh) / 2.0);
            let inside = |color| Shape::Rounded {
                x: ix + 1.0,
                y: iy + 1.0,
                w: fw,
                h: fh,
                radii: uniform(inner),
                color,
            };

            let mut shapes = vec![Shape::Rounded {
                x: ix,
                y: iy,
                w: iw,
                h: ih,
                radii: uniform(radius),
                color: ring,
            }];
            // Clears the ring out of the middle, so a translucent fill doesn't
            // show it through.
            if !tab.floating {
                shapes.push(inside(palette.bg));
            }
            shapes.push(inside(fill));
            shapes
        }
        None => vec![Shape::Rounded {
            x: ix,
            y: iy,
            w: iw,
            h: ih,
            radii: uniform(radius),
            color: fill,
        }],
    }
}

fn underline(
    x: f32,
    w: f32,
    radius: f32,
    palette: &Palette,
    tab: &TabVisual,
) -> Vec<Shape> {
    let mut shapes = Vec::new();

    // Only `tabs-fill` and `tabs-active-fill` give this style a body at all.
    if let Some(fill) = tab
        .custom
        .is_none()
        .then(|| body_fill(palette, tab))
        .flatten()
    {
        let (ix, iy, iw, ih, _) = island_rect(x, w);
        shapes.push(Shape::Rounded {
            x: ix,
            y: iy,
            w: iw,
            h: ih,
            radii: uniform(fitted_radius(radius, iw, ih)),
            color: fill,
        });
    }

    if let Some(color) = accent_line(palette, tab) {
        shapes.push(Shape::Rect {
            x: x + UNDERLINE_INSET_X,
            y: ISLAND_HEIGHT - UNDERLINE_OFFSET_Y,
            w: (w - UNDERLINE_INSET_X * 2.0).max(0.0),
            h: ACCENT_LINE_HEIGHT,
            color,
        });
    }
    shapes
}

fn top_accent(x: f32, w: f32, palette: &Palette, tab: &TabVisual) -> Vec<Shape> {
    let mut shapes = Vec::new();

    if let Some(color) = tab
        .custom
        .is_none()
        .then(|| body_fill(palette, tab))
        .flatten()
    {
        shapes.push(Shape::Rect {
            x,
            y: 0.0,
            w,
            h: ISLAND_HEIGHT,
            color,
        });
    }

    if tab.active || tab.floating {
        let color = tinted(palette.bg, EDGE_TINT);
        for edge_x in [x, x + w - 1.0] {
            shapes.push(Shape::Rect {
                x: edge_x,
                y: 0.0,
                w: 1.0,
                h: ISLAND_HEIGHT,
                color,
            });
        }
    }

    if let Some(color) = accent_line(palette, tab) {
        shapes.push(Shape::Rect {
            x,
            y: 0.0,
            w,
            h: ACCENT_LINE_HEIGHT,
            color,
        });
    }
    shapes
}

fn connected(
    x: f32,
    w: f32,
    radius: f32,
    palette: &Palette,
    tab: &TabVisual,
) -> Vec<Shape> {
    let Some(color) = body_fill(palette, tab) else {
        return Vec::new();
    };

    let h = ISLAND_HEIGHT - CONNECTED_TOP;
    let (x, w) = (x + TAB_GAP / 4.0, (w - TAB_GAP / 2.0).max(0.0));
    let top = fitted_radius(radius, w, h);
    vec![Shape::Rounded {
        x,
        y: CONNECTED_TOP,
        w,
        h,
        radii: [top, top, 0.0, 0.0],
        color,
    }]
}

/// A concave corner, filled in by fanning from the point it curves around.
fn flare(corner_x: f32, side: f32, size: f32, color: [f32; 4]) -> Shape {
    let bottom = ISLAND_HEIGHT;
    let center = (corner_x + side * size, bottom - size);
    let mut points = vec![(corner_x, bottom)];
    for step in 0..=CURVE_FLARE_STEPS {
        let t = step as f32 / CURVE_FLARE_STEPS as f32 * std::f32::consts::FRAC_PI_2;
        points.push((center.0 - side * size * t.cos(), center.1 + size * t.sin()));
    }
    points.reverse();
    Shape::Polygon { points, color }
}

fn browser_curve(
    x: f32,
    w: f32,
    radius: f32,
    palette: &Palette,
    tab: &TabVisual,
) -> Vec<Shape> {
    let mut shapes = connected(x, w, radius, palette, tab);

    if let (Some(color), false) = (body_fill(palette, tab), shapes.is_empty()) {
        let size = radius.clamp(0.0, CURVE_MAX_FLARE);
        if size > 0.0 {
            let inset = TAB_GAP / 4.0;
            shapes.push(flare(x + inset, -1.0, size, color));
            shapes.push(flare(x + w - inset, 1.0, size, color));
        }
    }

    if tab.divider {
        shapes.push(Shape::Rect {
            x: x + w - 0.5,
            y: SEPARATOR_INSET_Y,
            w: 1.0,
            h: ISLAND_HEIGHT - SEPARATOR_INSET_Y * 2.0,
            color: tinted(palette.strip.unwrap_or(palette.bg), POWERLINE_INACTIVE_TINT),
        });
    }
    shapes
}

/// A tab with a convex outline that ends in a tip over the next tab. The gap
/// is the same outline in the strip color, pushed out from under it.
fn powerline(
    x: f32,
    w: f32,
    palette: &Palette,
    tab: &TabVisual,
    outline: impl Fn(f32, f32, f32, f32) -> Vec<(f32, f32)>,
) -> Vec<Shape> {
    let Some(fill) = body_fill(palette, tab) else {
        return Vec::new();
    };
    let top = POWERLINE_INSET_Y;
    let bottom = ISLAND_HEIGHT - POWERLINE_INSET_Y;

    vec![
        Shape::Polygon {
            points: outline(x + POWERLINE_GAP, w, top, bottom),
            color: opaque(palette.strip.unwrap_or(palette.bg)),
        },
        Shape::Polygon {
            points: outline(x, w, top, bottom),
            color: over(opaque(palette.bg), fill),
        },
    ]
}

fn round_cap(x: f32, w: f32, top: f32, bottom: f32) -> Vec<(f32, f32)> {
    let middle = (top + bottom) / 2.0;
    let half = (bottom - top) / 2.0;

    let mut points = vec![(x, top), (x, bottom)];
    for step in 0..=POWERLINE_CAP_STEPS {
        let t = step as f32 / POWERLINE_CAP_STEPS as f32 * std::f32::consts::PI;
        points.push((x + w + POWERLINE_TIP * t.sin(), middle + half * t.cos()));
    }
    points
}

fn powerline_slanted(x: f32, w: f32, palette: &Palette, tab: &TabVisual) -> Vec<Shape> {
    let Some(fill) = body_fill(palette, tab) else {
        return Vec::new();
    };
    let top = POWERLINE_INSET_Y;
    let bottom = ISLAND_HEIGHT - POWERLINE_INSET_Y;
    let w = (w - POWERLINE_GAP).max(0.0);

    vec![Shape::Polygon {
        points: vec![
            (x + POWERLINE_SLANT, top),
            (x, bottom),
            (x + w, bottom),
            (x + w + POWERLINE_SLANT, top),
        ],
        color: over(opaque(palette.bg), fill),
    }]
}

/// How far past its slot the last tab reaches.
pub fn overhang(style: TabStyle) -> f32 {
    match style {
        TabStyle::PowerlineAngled | TabStyle::PowerlineRound => {
            POWERLINE_TIP + POWERLINE_GAP
        }
        TabStyle::PowerlineSlanted => POWERLINE_SLANT,
        _ => 0.0,
    }
}

/// Where a title sits relative to the middle of its slot, to look centered in
/// the shape around it.
pub fn title_shift(style: TabStyle) -> f32 {
    match style {
        TabStyle::PowerlineAngled | TabStyle::PowerlineRound => POWERLINE_TIP / 2.0,
        TabStyle::PowerlineSlanted => POWERLINE_SLANT / 2.0,
        _ => 0.0,
    }
}

/// Whether a tab's close button is showing.
pub fn close_visible(
    mode: TabCloseButton,
    tab: usize,
    current: usize,
    hovered: Option<usize>,
) -> bool {
    match mode {
        TabCloseButton::Active => tab == current,
        TabCloseButton::Hover => hovered == Some(tab),
        TabCloseButton::Always => true,
        TabCloseButton::Never => false,
    }
}

pub fn paint(sugarloaf: &mut Sugarloaf, shapes: &[Shape], order: u8) {
    for shape in shapes {
        match shape {
            Shape::Rect { x, y, w, h, color } => {
                sugarloaf.rect(None, *x, *y, *w, *h, *color, 0.0, order);
            }
            Shape::Rounded {
                x,
                y,
                w,
                h,
                radii,
                color,
            } => {
                if radii.iter().all(|radius| radius == &radii[0]) {
                    sugarloaf.rounded_rect(
                        None, *x, *y, *w, *h, *color, 0.05, radii[0], order,
                    );
                } else {
                    sugarloaf.quad(None, *x, *y, *w, *h, *color, *radii, 0.05, order);
                }
            }
            Shape::Polygon { points, color } => {
                sugarloaf.polygon_with_order(points, 0.05, *color, order);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DARK: [f32; 4] = [0.05, 0.05, 0.06, 1.0];
    const LIGHT: [f32; 4] = [0.95, 0.95, 0.95, 1.0];
    const ACCENT: [f32; 4] = [0.2, 0.8, 0.6, 1.0];
    const ALL: [TabStyle; 8] = [
        TabStyle::Islands,
        TabStyle::Underline,
        TabStyle::TopAccent,
        TabStyle::Connected,
        TabStyle::BrowserCurve,
        TabStyle::PowerlineAngled,
        TabStyle::PowerlineSlanted,
        TabStyle::PowerlineRound,
    ];

    fn appearance(style: TabStyle) -> TabAppearance {
        TabAppearance {
            style,
            accent: ACCENT,
            ..TabAppearance::default()
        }
    }

    fn shapes(style: TabStyle, tab: TabVisual) -> Vec<Shape> {
        let palette = Palette::new(&appearance(style), DARK);
        tab_shapes(style, 100.0, 120.0, 6.0, &palette, &tab)
    }

    fn active() -> TabVisual {
        TabVisual {
            active: true,
            ..TabVisual::default()
        }
    }

    /// The renderer fans polygons from their first point, which only comes
    /// out right on a convex outline.
    fn is_convex(points: &[(f32, f32)]) -> bool {
        let mut sign = 0.0f32;
        for i in 0..points.len() {
            let (ax, ay) = points[i];
            let (bx, by) = points[(i + 1) % points.len()];
            let (cx, cy) = points[(i + 2) % points.len()];
            let cross = (bx - ax) * (cy - by) - (by - ay) * (cx - bx);
            if cross.abs() < 0.0001 {
                continue;
            }
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
        true
    }

    #[test]
    fn powerline_outlines_are_convex() {
        for style in [
            TabStyle::PowerlineAngled,
            TabStyle::PowerlineSlanted,
            TabStyle::PowerlineRound,
        ] {
            for tab in [active(), TabVisual::default()] {
                for shape in shapes(style, tab) {
                    let Shape::Polygon { points, .. } = shape else {
                        panic!("{style:?} should be all polygons");
                    };
                    assert!(is_convex(&points), "{style:?}: {points:?}");
                }
            }
        }
    }

    /// A flare's outline curves inward, but it's still fanned from a point
    /// that sees all of it, so nothing crosses.
    #[test]
    fn flares_stay_in_their_corner() {
        for (shape, left) in [
            (flare(100.0, -1.0, 8.0, DARK), true),
            (flare(200.0, 1.0, 8.0, DARK), false),
        ] {
            let Shape::Polygon { points, .. } = shape else {
                panic!("a flare is a polygon");
            };
            for (x, y) in points {
                assert!((ISLAND_HEIGHT - 8.001..=ISLAND_HEIGHT + 0.001).contains(&y));
                if left {
                    assert!((92.0 - 0.001..=100.001).contains(&x), "{x}");
                } else {
                    assert!((199.999..=208.001).contains(&x), "{x}");
                }
            }
        }
    }

    #[test]
    fn every_style_marks_the_active_tab() {
        for style in ALL {
            assert_ne!(
                shapes(style, active()),
                shapes(style, TabVisual::default()),
                "{style:?}"
            );
        }
    }

    #[test]
    fn connected_styles_reach_the_terminal() {
        for style in [TabStyle::Connected, TabStyle::BrowserCurve] {
            let Shape::Rounded {
                y, h, radii, color, ..
            } = shapes(style, active())[0]
            else {
                panic!("expected the body first");
            };
            assert_eq!(y + h, ISLAND_HEIGHT);
            assert_eq!(radii[2], 0.0);
            assert_eq!(radii[3], 0.0);
            // Same color as the window, or it wouldn't read as one surface.
            assert_eq!(color, DARK);
        }
    }

    #[test]
    fn strip_is_tinted_only_for_styles_that_need_one() {
        for style in ALL {
            let palette = Palette::new(&appearance(style), DARK);
            assert_eq!(palette.strip.is_some(), style.has_own_strip(), "{style:?}");
        }
        let palette = Palette::new(&appearance(TabStyle::Connected), DARK);
        assert_ne!(palette.strip, Some(DARK));
    }

    #[test]
    fn configured_colors_win() {
        let colors = TabAppearance {
            fill: Some([0.1, 0.2, 0.3, 1.0]),
            active_fill: Some([0.4, 0.5, 0.6, 1.0]),
            strip: Some([0.7, 0.8, 0.9, 1.0]),
            ..appearance(TabStyle::Islands)
        };
        let palette = Palette::new(&colors, DARK);
        assert_eq!(palette.inactive, colors.fill);
        assert_eq!(palette.active, colors.active_fill);
        assert_eq!(palette.strip, colors.strip);
    }

    #[test]
    fn custom_color_marks_styles_without_a_body() {
        let custom = [0.9, 0.1, 0.1, 1.0];
        let tab = TabVisual {
            custom: Some(custom),
            active: true,
            ..TabVisual::default()
        };
        for style in [TabStyle::Underline, TabStyle::TopAccent] {
            assert!(
                shapes(style, tab).iter().any(|shape| matches!(
                    shape,
                    Shape::Rect { color, .. } if *color == custom
                )),
                "{style:?}"
            );
        }
    }

    #[test]
    fn inactive_custom_color_is_muted() {
        let tab = TabVisual {
            custom: Some(ACCENT),
            ..TabVisual::default()
        };
        let palette = Palette::new(&appearance(TabStyle::Islands), DARK);
        assert_eq!(body_fill(&palette, &tab).unwrap()[3], INACTIVE_CUSTOM_MUTE);
    }

    #[test]
    fn floating_islands_hide_what_they_pass_over() {
        let floating = TabVisual {
            active: true,
            floating: true,
            ..TabVisual::default()
        };
        let palette = Palette::new(&appearance(TabStyle::Islands), DARK);
        let shapes = tab_shapes(TabStyle::Islands, 0.0, 100.0, 6.0, &palette, &floating);
        let Shape::Rounded { color, .. } = shapes[0] else {
            panic!("islands are rounded");
        };
        assert_eq!(color[3], 1.0);
    }

    #[test]
    fn radius_is_honoured_and_fits() {
        let palette = Palette::new(&appearance(TabStyle::Islands), DARK);
        let tab = active();
        let shapes =
            |radius| tab_shapes(TabStyle::Islands, 0.0, 200.0, radius, &palette, &tab);
        assert!(matches!(
            shapes(0.0)[0],
            Shape::Rounded {
                radii: [0.0, ..],
                ..
            }
        ));
        assert!(matches!(
            shapes(4.0)[0],
            Shape::Rounded {
                radii: [4.0, ..],
                ..
            }
        ));
        // Half the height is a pill, and anything beyond it would overlap.
        let (_, _, _, ih, _) = island_rect(0.0, 200.0);
        let Shape::Rounded { radii, .. } = shapes(500.0)[0] else {
            panic!("islands are rounded");
        };
        assert_eq!(radii[0], ih / 2.0);
    }

    #[test]
    fn light_theme_keeps_the_island_outline() {
        let palette = Palette::new(&appearance(TabStyle::Islands), LIGHT);
        let shapes = tab_shapes(TabStyle::Islands, 0.0, 200.0, 6.0, &palette, &active());
        assert_eq!(shapes.len(), 3);
    }

    #[test]
    fn close_button_modes() {
        use TabCloseButton::*;
        assert!(close_visible(Active, 2, 2, None));
        assert!(!close_visible(Active, 1, 2, Some(1)));
        assert!(close_visible(Hover, 1, 2, Some(1)));
        assert!(!close_visible(Hover, 2, 2, Some(1)));
        assert!(close_visible(Always, 0, 2, None));
        assert!(!close_visible(Never, 2, 2, Some(2)));
    }

    #[test]
    fn readable_only_steps_in_when_it_must() {
        let white = [1.0, 1.0, 1.0, 1.0];
        assert_eq!(readable(white, DARK), white);
        assert_eq!(readable(white, LIGHT), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(readable([0.1, 0.1, 0.1, 1.0], DARK), white);
    }

    #[test]
    fn only_tips_overhang() {
        assert_eq!(overhang(TabStyle::Islands), 0.0);
        assert!(overhang(TabStyle::PowerlineAngled) > POWERLINE_TIP);
        assert_eq!(title_shift(TabStyle::Underline), 0.0);
    }
}
