//! Support for a title bar drawn by the application inside the client area.
//!
//! The window keeps its usual `WS_OVERLAPPEDWINDOW` styles, so Snap, the
//! minimize and maximize animations, the drop shadow and the rounded corners
//! all stay with the OS. `WM_NCCALCSIZE` only takes the caption out of the
//! non-client area, and `WM_NCHITTEST` then answers for the strip the
//! application paints there: reporting `HTCAPTION`, `HTTOP` and the caption
//! button codes is what gets window dragging, double-click maximize and the
//! Windows 11 Snap Layouts flyout from the system instead of imitating them.

use std::mem;
use std::time::Instant;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::HMONITOR;
use windows_sys::Win32::UI::Shell::{
    SHAppBarMessage, ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETAUTOHIDEBAREX,
    ABM_GETSTATE, ABS_AUTOHIDE, APPBARDATA,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, HTCAPTION, HTCLIENT, HTCLOSE, HTMAXBUTTON, HTMINBUTTON, HTSYSMENU,
    HTTOP, HTTOPLEFT, HTTOPRIGHT, SC_CLOSE, SC_MAXIMIZE, SC_MINIMIZE, SC_RESTORE,
    SM_CXPADDEDBORDER, SM_CYSIZEFRAME,
};

use super::monitor;
use super::util;
use crate::event::TitlebarButton;
use crate::platform::windows::TitlebarLayout;

/// How far a maximized window stays off a screen edge that holds an
/// auto-hide taskbar. A window covering the whole monitor reads as fullscreen
/// to the shell, and the taskbar would then never slide back in.
const AUTOHIDE_TASKBAR_GAP: i32 = 2;

/// What the window procedure tracks for an application-drawn title bar.
#[derive(Default)]
pub(crate) struct TitlebarState {
    pub layout: TitlebarLayout,
    /// The caption button under the pointer.
    pub hovered: Option<TitlebarButton>,
    /// The caption button the left button went down on.
    pub pressed: Option<TitlebarButton>,
    /// The hover state last sent to the application.
    pub reported: (Option<TitlebarButton>, bool),
    /// When a left click last opened the system menu. The menu loop swallows
    /// the double-click message, so a second click has to be matched here.
    pub system_menu_opened: Option<Instant>,
}

impl TitlebarState {
    /// Record a change and return the state to send, if there is news.
    pub fn take_report(&mut self) -> Option<(Option<TitlebarButton>, bool)> {
        let current = (
            self.hovered,
            self.hovered.is_some() && self.hovered == self.pressed,
        );
        if current == self.reported {
            return None;
        }
        self.reported = current;
        Some(current)
    }
}

/// The `WM_NCHITTEST` code for a point in client coordinates. `width` is the
/// client width. `resize_border` is the height of the top edge that resizes
/// the window: zero while it can't be resized, such as when maximized.
pub(crate) fn hit_test(
    layout: &TitlebarLayout,
    x: i32,
    y: i32,
    width: i32,
    resize_border: i32,
) -> u32 {
    // The left, right and bottom edges are real frame, but the caption was
    // given to the client area and took the top one with it.
    if y < resize_border {
        return if x < resize_border {
            HTTOPLEFT
        } else if x >= width - resize_border {
            HTTOPRIGHT
        } else {
            HTTOP
        };
    }

    if y >= layout.height as i32 {
        return HTCLIENT;
    }

    let button_width = layout.button_width as i32;
    if button_width > 0 {
        let buttons_left = width - button_width * 3;
        if x >= buttons_left {
            return match (x - buttons_left) / button_width {
                0 => HTMINBUTTON,
                1 => HTMAXBUTTON,
                _ => HTCLOSE,
            };
        }
    }

    if x < layout.icon_width as i32 {
        return HTSYSMENU;
    }

    if layout
        .client_regions
        .iter()
        .any(|&(left, right)| x >= left && x < right)
    {
        return HTCLIENT;
    }

    HTCAPTION
}

/// The caption button a `WM_NC*` hit-test code names.
pub(crate) fn button_from_hit(hit: usize) -> Option<TitlebarButton> {
    match hit as u32 {
        HTMINBUTTON => Some(TitlebarButton::Minimize),
        HTMAXBUTTON => Some(TitlebarButton::Maximize),
        HTCLOSE => Some(TitlebarButton::Close),
        _ => None,
    }
}

/// The `WM_SYSCOMMAND` a click on a caption button stands for.
pub(crate) fn system_command(button: TitlebarButton, maximized: bool) -> usize {
    (match button {
        TitlebarButton::Minimize => SC_MINIMIZE,
        TitlebarButton::Maximize if maximized => SC_RESTORE,
        TitlebarButton::Maximize => SC_MAXIMIZE,
        TitlebarButton::Close => SC_CLOSE,
    }) as usize
}

/// The height of the window's sizing frame, which is also how far a maximized
/// window hangs off the top of the screen.
pub(crate) fn frame_height(window: HWND) -> i32 {
    unsafe {
        match *util::GET_SYSTEM_METRICS_FOR_DPI {
            Some(metrics) => {
                let dpi = match *util::GET_DPI_FOR_WINDOW {
                    Some(get_dpi) => get_dpi(window),
                    None => 96,
                };
                metrics(SM_CYSIZEFRAME, dpi) + metrics(SM_CXPADDEDBORDER, dpi)
            }
            None => {
                GetSystemMetrics(SM_CYSIZEFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER)
            }
        }
    }
}

/// Pull a maximized client rectangle back from every monitor edge that holds
/// an auto-hide taskbar.
pub(crate) fn leave_room_for_autohide_taskbars(monitor: HMONITOR, client: &mut RECT) {
    let Ok(info) = monitor::get_monitor_info(monitor) else {
        return;
    };

    unsafe {
        let mut data: APPBARDATA = mem::zeroed();
        data.cbSize = mem::size_of::<APPBARDATA>() as u32;
        if SHAppBarMessage(ABM_GETSTATE, &mut data) as u32 & ABS_AUTOHIDE == 0 {
            return;
        }

        data.rc = info.monitorInfo.rcMonitor;
        let mut has_bar = |edge: u32| {
            data.uEdge = edge;
            SHAppBarMessage(ABM_GETAUTOHIDEBAREX, &mut data) != 0
        };

        if has_bar(ABE_TOP) {
            client.top += AUTOHIDE_TASKBAR_GAP;
        }
        if has_bar(ABE_BOTTOM) {
            client.bottom -= AUTOHIDE_TASKBAR_GAP;
        }
        if has_bar(ABE_LEFT) {
            client.left += AUTOHIDE_TASKBAR_GAP;
        }
        if has_bar(ABE_RIGHT) {
            client.right -= AUTOHIDE_TASKBAR_GAP;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: i32 = 1000;
    const BORDER: i32 = 8;

    fn layout() -> TitlebarLayout {
        TitlebarLayout {
            height: 38,
            icon_width: 40,
            button_width: 46,
            client_regions: vec![(40, 400)],
        }
    }

    #[test]
    fn top_edge_resizes_over_everything_else() {
        let layout = layout();
        assert_eq!(hit_test(&layout, 500, 0, WIDTH, BORDER), HTTOP);
        assert_eq!(hit_test(&layout, 100, 7, WIDTH, BORDER), HTTOP);
        assert_eq!(hit_test(&layout, 2, 2, WIDTH, BORDER), HTTOPLEFT);
        assert_eq!(hit_test(&layout, 998, 2, WIDTH, BORDER), HTTOPRIGHT);
        // Over the buttons too, or the top right corner couldn't be grabbed.
        assert_eq!(hit_test(&layout, 900, 2, WIDTH, BORDER), HTTOP);
    }

    /// A maximized window has no top edge to grab, so the strip reaches the
    /// top of the screen and a throw at the corner lands on the close button.
    #[test]
    fn nothing_resizes_while_maximized() {
        let layout = layout();
        assert_eq!(hit_test(&layout, 999, 0, WIDTH, 0), HTCLOSE);
        assert_eq!(hit_test(&layout, 500, 0, WIDTH, 0), HTCAPTION);
        assert_eq!(hit_test(&layout, 0, 0, WIDTH, 0), HTSYSMENU);
    }

    #[test]
    fn caption_buttons_sit_on_the_right() {
        let layout = layout();
        assert_eq!(hit_test(&layout, WIDTH - 1, 20, WIDTH, BORDER), HTCLOSE);
        assert_eq!(hit_test(&layout, WIDTH - 46, 20, WIDTH, BORDER), HTCLOSE);
        assert_eq!(
            hit_test(&layout, WIDTH - 47, 20, WIDTH, BORDER),
            HTMAXBUTTON
        );
        assert_eq!(
            hit_test(&layout, WIDTH - 92, 20, WIDTH, BORDER),
            HTMAXBUTTON
        );
        assert_eq!(
            hit_test(&layout, WIDTH - 93, 20, WIDTH, BORDER),
            HTMINBUTTON
        );
        assert_eq!(
            hit_test(&layout, WIDTH - 138, 20, WIDTH, BORDER),
            HTMINBUTTON
        );
        assert_eq!(hit_test(&layout, WIDTH - 139, 20, WIDTH, BORDER), HTCAPTION);
    }

    #[test]
    fn tabs_stay_with_the_application_and_the_rest_drags() {
        let layout = layout();
        assert_eq!(hit_test(&layout, 39, 20, WIDTH, BORDER), HTSYSMENU);
        assert_eq!(hit_test(&layout, 40, 20, WIDTH, BORDER), HTCLIENT);
        assert_eq!(hit_test(&layout, 399, 20, WIDTH, BORDER), HTCLIENT);
        assert_eq!(hit_test(&layout, 400, 20, WIDTH, BORDER), HTCAPTION);
    }

    #[test]
    fn below_the_strip_is_client_area() {
        let layout = layout();
        assert_eq!(hit_test(&layout, 500, 37, WIDTH, BORDER), HTCAPTION);
        assert_eq!(hit_test(&layout, 500, 38, WIDTH, BORDER), HTCLIENT);
        assert_eq!(hit_test(&layout, WIDTH - 1, 38, WIDTH, BORDER), HTCLIENT);
    }

    /// Until the application reports a layout there is no strip to speak of,
    /// but the window still has to be resizable from its top edge.
    #[test]
    fn empty_layout_is_all_client_area() {
        let layout = TitlebarLayout::default();
        assert_eq!(hit_test(&layout, 500, 20, WIDTH, BORDER), HTCLIENT);
        assert_eq!(hit_test(&layout, 500, 0, WIDTH, BORDER), HTTOP);
    }

    #[test]
    fn hover_is_reported_once_per_change() {
        let mut state = TitlebarState::default();
        assert_eq!(state.take_report(), None);

        state.hovered = Some(TitlebarButton::Close);
        assert_eq!(
            state.take_report(),
            Some((Some(TitlebarButton::Close), false))
        );
        assert_eq!(state.take_report(), None);

        state.pressed = Some(TitlebarButton::Close);
        assert_eq!(
            state.take_report(),
            Some((Some(TitlebarButton::Close), true))
        );

        // Sliding off a pressed button lets it go until the pointer returns.
        state.hovered = Some(TitlebarButton::Maximize);
        assert_eq!(
            state.take_report(),
            Some((Some(TitlebarButton::Maximize), false))
        );
    }

    #[test]
    fn maximize_button_toggles() {
        assert_eq!(
            system_command(TitlebarButton::Maximize, false),
            SC_MAXIMIZE as usize
        );
        assert_eq!(
            system_command(TitlebarButton::Maximize, true),
            SC_RESTORE as usize
        );
    }
}
