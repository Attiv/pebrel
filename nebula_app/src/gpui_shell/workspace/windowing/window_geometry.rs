//! Persisted normal-window placement and native bounds capture.
use std::rc::Rc;

use gpui::{App, AppContext as _, Bounds, Pixels, PlatformDisplay, Size, Window, point, px};

use super::{NebulaWorkspace, WindowRegistry, WindowRole};
use crate::session::WindowState;

pub(super) fn validated_state(state: Option<WindowState>) -> Option<WindowState> {
    // A native window cannot produce a usable zero-sized normal rectangle.
    // Bound corrupt snapshots before converting their unsigned size to Pixels.
    state
        .filter(|state| (1..=32_768).contains(&state.width) && (1..=32_768).contains(&state.height))
}

pub(super) fn restored_display(
    cx: &App,
    state: Option<WindowState>,
) -> Option<Rc<dyn PlatformDisplay>> {
    let displays = cx.displays();
    let bounds = displays.iter().map(|display| display.visible_bounds()).collect::<Vec<_>>();
    matching_display(state, &bounds)
        .and_then(|index| displays.get(index).cloned())
        .or_else(|| cx.primary_display())
}

fn matching_display(state: Option<WindowState>, displays: &[Bounds<Pixels>]) -> Option<usize> {
    let state = validated_state(state)?;
    let (Some(x), Some(y)) = (state.x, state.y) else { return None };
    let saved = Bounds::new(
        point(px(x as f32), px(y as f32)),
        gpui::size(px(state.width as f32), px(state.height as f32)),
    );
    displays
        .iter()
        .enumerate()
        .filter_map(|(index, display)| {
            useful_intersection(saved, *display).map(|area| (index, area))
        })
        .max_by(|first, second| first.1.total_cmp(&second.1))
        .map(|(index, _)| index)
}

fn useful_intersection(saved: Bounds<Pixels>, display: Bounds<Pixels>) -> Option<f32> {
    let width = (saved.origin.x + saved.size.width).min(display.origin.x + display.size.width)
        - saved.origin.x.max(display.origin.x);
    let height = (saved.origin.y + saved.size.height).min(display.origin.y + display.size.height)
        - saved.origin.y.max(display.origin.y);
    // A sliver cannot expose a usable title-bar grip; treat it as offscreen.
    (width >= px(32.0) && height >= px(32.0)).then_some(f32::from(width) * f32::from(height))
}

pub(super) fn restore_bounds(
    state: Option<crate::session::WindowState>,
    visible: Bounds<Pixels>,
    preferred: Size<Pixels>,
) -> Bounds<Pixels> {
    let fitted = preferred.min(&visible.size);
    let centered = || {
        Bounds::new(
            point(
                visible.origin.x + (visible.size.width - fitted.width) / 2.0,
                visible.origin.y + (visible.size.height - fitted.height) / 2.0,
            ),
            fitted,
        )
    };
    let Some(state) = state else { return centered() };
    let (Some(x), Some(y)) = (state.x, state.y) else { return centered() };
    let saved = Bounds::new(point(px(x as f32), px(y as f32)), fitted);
    let visible_right = visible.origin.x + visible.size.width;
    let visible_bottom = visible.origin.y + visible.size.height;
    if useful_intersection(saved, visible).is_none() {
        return centered();
    }
    Bounds::new(
        point(
            saved.origin.x.max(visible.origin.x).min(visible_right - fitted.width),
            saved.origin.y.max(visible.origin.y).min(visible_bottom - fitted.height),
        ),
        fitted,
    )
}

pub(super) fn capture(
    previous: Option<WindowState>,
    role: WindowRole,
    window: &Window,
) -> Option<WindowState> {
    capture_bounds(
        previous,
        role,
        window.is_fullscreen(),
        window.is_maximized(),
        window.window_bounds().get_bounds(),
    )
}

fn capture_bounds(
    previous: Option<WindowState>,
    role: WindowRole,
    fullscreen: bool,
    maximized: bool,
    bounds: Bounds<Pixels>,
) -> Option<WindowState> {
    if role != WindowRole::Regular || fullscreen {
        return previous;
    }
    if maximized {
        return previous.map(|mut state| {
            state.maximized = true;
            state
        });
    }
    Some(WindowState {
        x: Some(f32::from(bounds.origin.x).round() as i32),
        y: Some(f32::from(bounds.origin.y).round() as i32),
        width: f32::from(bounds.size.width).round().max(1.0) as u32,
        height: f32::from(bounds.size.height).round().max(1.0) as u32,
        maximized: false,
    })
}

pub(super) fn capture_all(cx: &mut App) {
    for entry in cx.global::<WindowRegistry>().entries.clone() {
        if entry.role != WindowRole::Regular {
            continue;
        }
        let workspace = entry.workspace;
        let _ = entry.handle.update(cx, move |_, window, cx| {
            let _ = workspace.update(cx, |workspace, _| workspace.record_window_bounds(window));
        });
    }
}

impl NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn record_window_bounds(&mut self, window: &Window) {
        self.window_state = capture(self.window_state, self.window_role, window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    fn state(x: i32, y: i32) -> WindowState {
        WindowState { x: Some(x), y: Some(y), width: 1200, height: 720, maximized: false }
    }

    fn screen(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    #[test]
    fn saved_secondary_display_origin_selects_that_display() {
        let displays = [screen(0.0, 0.0, 1920.0, 1040.0), screen(1920.0, 0.0, 1920.0, 1040.0)];
        assert_eq!(matching_display(Some(state(2100, 80)), &displays), Some(1));
        assert_eq!(
            matching_display(
                Some(state(-1700, 80)),
                &[screen(-1920.0, 0.0, 1920.0, 1040.0), displays[0]]
            ),
            Some(0)
        );
    }

    #[test]
    fn largest_useful_intersection_wins_and_removed_displays_do_not_match() {
        let displays = [screen(0.0, 0.0, 1920.0, 1040.0), screen(1920.0, 0.0, 1920.0, 1040.0)];
        assert_eq!(matching_display(Some(state(1500, 80)), &displays), Some(1));
        assert_eq!(matching_display(Some(state(4000, 80)), &displays), None);
        assert_eq!(matching_display(Some(state(-1190, 80)), &displays), None);
        assert_eq!(matching_display(None, &displays), None);
        assert_eq!(matching_display(Some(state(100, 80)), &[]), None);
    }

    #[test]
    fn fitting_preserves_reachable_secondary_origin_and_centers_offscreen_bounds() {
        let visible = screen(1920.0, 40.0, 1920.0, 1000.0);
        let preferred = size(px(1200.0), px(720.0));
        assert_eq!(
            restore_bounds(Some(state(2100, 80)), visible, preferred),
            screen(2100.0, 80.0, 1200.0, 720.0)
        );
        assert_eq!(
            restore_bounds(Some(state(4000, 80)), visible, preferred),
            screen(2280.0, 180.0, 1200.0, 720.0)
        );
        assert_eq!(
            restore_bounds(Some(state(3500, 600)), visible, preferred),
            screen(2640.0, 320.0, 1200.0, 720.0)
        );
        assert_eq!(
            restore_bounds(Some(state(2100, 80)), screen(1920.0, 40.0, 800.0, 600.0), preferred),
            screen(1920.0, 40.0, 800.0, 600.0)
        );
        assert_eq!(
            restore_bounds(Some(state(-1190, 80)), screen(0.0, 0.0, 1920.0, 1040.0), preferred),
            screen(360.0, 160.0, 1200.0, 720.0),
        );
    }

    #[test]
    fn maximization_fullscreen_and_quick_terminal_preserve_the_normal_rectangle() {
        let normal = state(100, 80);
        let actual = screen(0.0, 0.0, 1920.0, 1080.0);
        let maximized =
            capture_bounds(Some(normal), WindowRole::Regular, false, true, actual).unwrap();
        assert!(maximized.maximized);
        assert_eq!(
            (maximized.x, maximized.y, maximized.width, maximized.height),
            (normal.x, normal.y, normal.width, normal.height)
        );
        assert_eq!(
            capture_bounds(Some(normal), WindowRole::Regular, true, false, actual),
            Some(normal)
        );
        assert_eq!(
            capture_bounds(Some(normal), WindowRole::QuickTerminal, false, false, actual),
            Some(normal)
        );
        assert_eq!(
            capture_bounds(
                Some(maximized),
                WindowRole::Regular,
                false,
                false,
                screen(120.0, 90.0, 1000.0, 700.0)
            ),
            Some(WindowState {
                x: Some(120),
                y: Some(90),
                width: 1000,
                height: 700,
                maximized: false
            })
        );
    }
}
