// The window icon and caption buttons, for a window whose title bar has moved
// into the client area (`navigation.tabs-in-titlebar`).
//
// All of this is paint. What a click on any of it does is decided by the
// system, from the hit test rio-window answers with `hit_layout`, so what's
// drawn here has to use the same pixel arithmetic as that layout.

use crate::renderer::island::{StripInsets, ISLAND_HEIGHT};
use rio_backend::sugarloaf::{
    ColorType, GraphicData, GraphicDataEntry, GraphicId, GraphicOverlay, Sugarloaf,
};
use rio_window::event::TitlebarButton;
use rio_window::platform::windows::TitlebarLayout;

const ICON_SLOT_WIDTH: f32 = ISLAND_HEIGHT;
const ICON_SIZE: f32 = 16.0;
/// The width Windows gives its own caption buttons.
const BUTTON_WIDTH: f32 = 46.0;
const BUTTON_COUNT: f32 = 3.0;
/// Room kept free between the last tab and the buttons however many tabs are
/// open, so that there's always somewhere to grab the window by.
const MIN_DRAG_WIDTH: f32 = 32.0;
const GLYPH_SIZE: f32 = 10.0;
/// How far the back square of the restore glyph peeks out.
const RESTORE_OFFSET: f32 = 2.0;
const GLYPH_ALPHA_UNFOCUSED: f32 = 0.45;
const HOVER_ALPHA: f32 = 0.1;
const PRESSED_ALPHA: f32 = 0.06;
/// The red Windows uses for a hovered close button.
const CLOSE_HOVER_FILL: [f32; 4] = [0.769, 0.169, 0.110, 1.0];
const CLOSE_PRESSED_FILL: [f32; 4] = [0.769, 0.169, 0.110, 0.85];
const CLOSE_HOVER_GLYPH: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

const FILL_ORDER: u8 = 20;
const GLYPH_ORDER: u8 = 21;

// Terminal images key their route id into the high bits of these, and panels
// count up from zero, so neither reaches the top of the range.
const ICON_IMAGE_KEY: u64 = u64::MAX;
const ICON_PANEL_ID: usize = usize::MAX;

const BUTTONS: [TitlebarButton; 3] = [
    TitlebarButton::Minimize,
    TitlebarButton::Maximize,
    TitlebarButton::Close,
];

/// What the tab strip has to stay clear of.
pub fn strip_insets() -> StripInsets {
    StripInsets {
        left: ICON_SLOT_WIDTH,
        right: BUTTON_WIDTH * BUTTON_COUNT + MIN_DRAG_WIDTH,
    }
}

#[inline]
fn button_width_px(scale: f32) -> u32 {
    (BUTTON_WIDTH * scale).round() as u32
}

/// Describe the title bar to the window, `tabs` being the logical span of the
/// strip that clicks should reach the application in.
pub fn hit_layout(scale: f32, tabs: Option<(f32, f32)>) -> TitlebarLayout {
    TitlebarLayout {
        height: (ISLAND_HEIGHT * scale).round() as u32,
        icon_width: (ICON_SLOT_WIDTH * scale).round() as u32,
        button_width: button_width_px(scale),
        client_regions: tabs
            .map(|(left, right)| {
                (
                    (left * scale).round() as i32,
                    (right * scale).round() as i32,
                )
            })
            .into_iter()
            .collect(),
    }
}

#[derive(Default)]
pub struct Titlebar {
    /// Whether this window has a title bar to draw right now.
    pub active: bool,
    pub maximized: bool,
    hovered: Option<TitlebarButton>,
    pressed: bool,
    /// Edge of the icon image last uploaded, in physical pixels.
    icon_size: u32,
}

impl Titlebar {
    /// Returns whether that changes what's on screen.
    pub fn set_buttons(
        &mut self,
        hovered: Option<TitlebarButton>,
        pressed: bool,
    ) -> bool {
        let changed = self.hovered != hovered || self.pressed != pressed;
        self.hovered = hovered;
        self.pressed = pressed;
        changed
    }

    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        window_width: f32,
        scale: f32,
        bg_color: [f32; 4],
        focused: bool,
    ) {
        sugarloaf.clear_image_overlays_for(ICON_PANEL_ID);
        if !self.active {
            return;
        }

        self.draw_icon(sugarloaf, scale);

        let button_width = button_width_px(scale);
        let first_button = window_width - (button_width * BUTTONS.len() as u32) as f32;
        let base = glyph_color(bg_color);
        for (index, button) in BUTTONS.into_iter().enumerate() {
            let x_px = first_button + (button_width * index as u32) as f32;
            self.draw_button(
                sugarloaf,
                button,
                x_px / scale,
                button_width as f32 / scale,
                scale,
                base,
                focused,
            );
        }
    }

    fn draw_icon(&mut self, sugarloaf: &mut Sugarloaf, scale: f32) {
        // Drawn one texel to the pixel: the image is rescaled here, once per
        // DPI, with a filter that holds up at this size.
        let size = (ICON_SIZE * scale).round() as u32;
        if self.icon_size != size {
            if !upload_icon(sugarloaf, size) {
                return;
            }
            self.icon_size = size;
        }

        let inset = ((ICON_SLOT_WIDTH - ICON_SIZE) / 2.0 * scale).round();
        sugarloaf.push_image_overlay(
            ICON_PANEL_ID,
            GraphicOverlay {
                image_id: ICON_IMAGE_KEY,
                x: inset,
                y: inset,
                width: size as f32,
                height: size as f32,
                z_index: 0,
                source_rect: GraphicOverlay::FULL_SOURCE_RECT,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_button(
        &self,
        sugarloaf: &mut Sugarloaf,
        button: TitlebarButton,
        x: f32,
        width: f32,
        scale: f32,
        base: [f32; 4],
        focused: bool,
    ) {
        let hovered = self.hovered == Some(button);
        let close = button == TitlebarButton::Close;

        if hovered {
            let fill = match (close, self.pressed) {
                (true, false) => CLOSE_HOVER_FILL,
                (true, true) => CLOSE_PRESSED_FILL,
                (false, false) => with_alpha(base, HOVER_ALPHA),
                (false, true) => with_alpha(base, PRESSED_ALPHA),
            };
            sugarloaf.rect(None, x, 0.0, width, ISLAND_HEIGHT, fill, 0.0, FILL_ORDER);
        }

        let color = if hovered && close {
            CLOSE_HOVER_GLYPH
        } else if focused || hovered {
            base
        } else {
            with_alpha(base, GLYPH_ALPHA_UNFOCUSED)
        };

        let glyph = Glyph {
            left: snap(x + (width - GLYPH_SIZE) / 2.0, scale),
            top: snap((ISLAND_HEIGHT - GLYPH_SIZE) / 2.0, scale),
            stroke: stroke_width(scale),
            color,
        };
        match button {
            TitlebarButton::Minimize => glyph.minimize(sugarloaf),
            TitlebarButton::Maximize if self.maximized => glyph.restore(sugarloaf, scale),
            TitlebarButton::Maximize => glyph.maximize(sugarloaf),
            TitlebarButton::Close => glyph.close(sugarloaf),
        }
    }
}

/// A caption button glyph: thin strokes on whole pixels, like the ones it sits
/// among in every other window.
struct Glyph {
    left: f32,
    top: f32,
    stroke: f32,
    color: [f32; 4],
}

impl Glyph {
    fn fill(&self, sugarloaf: &mut Sugarloaf, x: f32, y: f32, w: f32, h: f32) {
        sugarloaf.rect(None, x, y, w, h, self.color, 0.0, GLYPH_ORDER);
    }

    /// An outlined square. The sides stop short of the top and bottom strokes
    /// so a translucent color doesn't double up in the corners.
    fn square(&self, sugarloaf: &mut Sugarloaf, x: f32, y: f32, size: f32) {
        let stroke = self.stroke;
        let side = size - stroke * 2.0;
        self.fill(sugarloaf, x, y, size, stroke);
        self.fill(sugarloaf, x, y + size - stroke, size, stroke);
        self.fill(sugarloaf, x, y + stroke, stroke, side);
        self.fill(sugarloaf, x + size - stroke, y + stroke, stroke, side);
    }

    fn minimize(&self, sugarloaf: &mut Sugarloaf) {
        let y = self.top + GLYPH_SIZE / 2.0;
        self.fill(sugarloaf, self.left, y, GLYPH_SIZE, self.stroke);
    }

    fn maximize(&self, sugarloaf: &mut Sugarloaf) {
        self.square(sugarloaf, self.left, self.top, GLYPH_SIZE);
    }

    fn restore(&self, sugarloaf: &mut Sugarloaf, scale: f32) {
        let stroke = self.stroke;
        let offset = snap(RESTORE_OFFSET, scale);
        let front = GLYPH_SIZE - offset;
        self.square(sugarloaf, self.left, self.top + offset, front);

        // What shows of the square behind it, up and to the right.
        let back_left = self.left + offset;
        let right = self.left + GLYPH_SIZE - stroke;
        self.fill(sugarloaf, back_left, self.top, GLYPH_SIZE - offset, stroke);
        self.fill(sugarloaf, right, self.top + stroke, stroke, front - stroke);
        self.fill(
            sugarloaf,
            back_left,
            self.top + stroke,
            stroke,
            offset - stroke,
        );
        self.fill(
            sugarloaf,
            right + stroke - offset,
            self.top + front - stroke,
            offset - stroke,
            stroke,
        );
    }

    fn close(&self, sugarloaf: &mut Sugarloaf) {
        let (left, top) = (self.left, self.top);
        let (right, bottom) = (left + GLYPH_SIZE, top + GLYPH_SIZE);
        let stroke = self.stroke;
        let color = self.color;
        sugarloaf.line(left, top, right, bottom, stroke, 0.0, color, GLYPH_ORDER);
        sugarloaf.line(left, bottom, right, top, stroke, 0.0, color, GLYPH_ORDER);
    }
}

/// One device pixel per unit of scale, never less than one, in logical pixels.
#[inline]
fn stroke_width(scale: f32) -> f32 {
    scale.round().max(1.0) / scale
}

/// The nearest logical position that falls on a whole device pixel.
#[inline]
fn snap(value: f32, scale: f32) -> f32 {
    (value * scale).round() / scale
}

/// Black or white, whichever reads on the strip.
fn glyph_color(bg: [f32; 4]) -> [f32; 4] {
    let luminance = 0.2126 * bg[0] + 0.7152 * bg[1] + 0.0722 * bg[2];
    if luminance > 0.5 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [1.0, 1.0, 1.0, 1.0]
    }
}

#[inline]
fn with_alpha(color: [f32; 4], alpha: f32) -> [f32; 4] {
    [color[0], color[1], color[2], alpha]
}

fn upload_icon(sugarloaf: &mut Sugarloaf, size: u32) -> bool {
    let image = match image_rs::load_from_memory(crate::router::LOGO_ICON) {
        Ok(image) => image,
        Err(error) => {
            tracing::warn!("can't decode the window icon: {error}");
            return false;
        }
    };
    let pixels = image_rs::imageops::resize(
        &image.to_rgba8(),
        size,
        size,
        image_rs::imageops::FilterType::Lanczos3,
    )
    .into_raw();

    sugarloaf.image_data.insert(
        ICON_IMAGE_KEY,
        GraphicDataEntry::from_graphic_data(GraphicData {
            id: GraphicId::new(ICON_IMAGE_KEY),
            width: size as usize,
            height: size as usize,
            color_type: ColorType::Rgba,
            pixels,
            is_opaque: false,
            resize: None,
            display_width: None,
            display_height: None,
            transmit_time: std::time::Instant::now(),
        }),
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_keeps_a_grip_clear_of_the_buttons() {
        let insets = strip_insets();
        assert!(insets.right >= BUTTON_WIDTH * BUTTON_COUNT + MIN_DRAG_WIDTH);
        assert_eq!(insets.left, ICON_SLOT_WIDTH);
    }

    #[test]
    fn hit_layout_is_in_physical_pixels() {
        let layout = hit_layout(1.5, Some((38.0, 400.0)));
        assert_eq!(layout.height, 57);
        assert_eq!(layout.icon_width, 57);
        assert_eq!(layout.button_width, 69);
        assert_eq!(layout.client_regions, vec![(57, 600)]);
    }

    #[test]
    fn a_lone_title_leaves_the_whole_strip_to_drag() {
        assert!(hit_layout(1.0, None).client_regions.is_empty());
    }

    #[test]
    fn button_state_reports_changes_only() {
        let mut titlebar = Titlebar::default();
        assert!(!titlebar.set_buttons(None, false));
        assert!(titlebar.set_buttons(Some(TitlebarButton::Close), false));
        assert!(!titlebar.set_buttons(Some(TitlebarButton::Close), false));
        assert!(titlebar.set_buttons(Some(TitlebarButton::Close), true));
    }

    #[test]
    fn strokes_land_on_whole_pixels() {
        assert_eq!(stroke_width(1.0), 1.0);
        assert_eq!(stroke_width(2.0), 1.0);
        // A fractional scale still gets a whole device pixel, never none.
        assert_eq!(stroke_width(0.75) * 0.75, 1.0);
        assert_eq!(snap(14.2, 1.0), 14.0);
        assert_eq!(snap(14.2, 2.0), 14.0);
        assert_eq!(snap(14.3, 2.0), 14.5);
    }

    #[test]
    fn glyphs_contrast_with_the_strip() {
        assert_eq!(glyph_color([0.0, 0.0, 0.0, 1.0])[0], 1.0);
        assert_eq!(glyph_color([1.0, 1.0, 1.0, 1.0])[0], 0.0);
    }
}
