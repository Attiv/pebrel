//! Window boundaries share the same atomic document as the compatible flat tabs.
use super::{Session, WindowState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowLayout {
    tab_count: usize,
    active_tab: usize,
    active: bool,
    window: Option<WindowState>,
}

pub(crate) fn combine_sessions(
    sessions: impl IntoIterator<Item = (bool, Session)>,
) -> Option<Session> {
    let mut combined = None;
    for (active, session) in sessions {
        let combined = combined.get_or_insert_with(|| Session::new(0, Vec::new()));
        if combined.window.is_none() || active {
            combined.window = session.window.or(combined.window);
        }
        if active {
            combined.active_tab = combined.tabs.len().saturating_add(session.active_tab);
        }
        combined.window_layout.push(WindowLayout {
            tab_count: session.tabs.len(),
            active_tab: session.active_tab,
            active,
            window: session.window,
        });
        combined.boot_attempts = combined.boot_attempts.max(session.boot_attempts);
        combined.tabs.extend(session.tabs);
    }
    if let Some(session) = combined.as_mut() {
        session.active_tab = session.active_tab.min(session.tabs.len().saturating_sub(1));
    }
    combined
}

impl Session {
    /// Earlier combined snapshots stored geometry only in window boundaries.
    /// Keep an explicit top-level value authoritative for flat-list readers.
    pub(super) fn recover_window_geometry(&mut self) {
        if self.window.is_none() {
            self.window = self
                .window_layout
                .iter()
                .find(|layout| layout.active)
                .and_then(|layout| layout.window)
                .or_else(|| self.window_layout.iter().find_map(|layout| layout.window));
        }
    }

    /// Open the previously active window last so native activation restores it.
    /// Invalid boundaries are an error, never permission to drop or duplicate tabs.
    pub(crate) fn into_update_windows(self) -> std::io::Result<Vec<Session>> {
        if self.window_layout.is_empty() {
            return Ok(vec![self]);
        }
        let total = self
            .window_layout
            .iter()
            .try_fold(0usize, |count, layout| count.checked_add(layout.tab_count));
        if total != Some(self.tabs.len())
            || self.window_layout.len() > 32
            || self.window_layout.iter().filter(|layout| layout.active).count() > 1
            || self
                .window_layout
                .iter()
                .any(|layout| layout.active_tab > layout.tab_count.saturating_sub(1))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid saved window boundaries",
            ));
        }
        let mut tabs = self.tabs.into_iter();
        let mut windows = Vec::new();
        let mut active = None;
        for layout in self.window_layout {
            let mut window =
                Session::new(layout.active_tab, tabs.by_ref().take(layout.tab_count).collect());
            window.boot_attempts = self.boot_attempts;
            window.clean_exit = self.clean_exit;
            window.window = layout.window;
            if layout.active {
                active = Some(window);
            } else {
                windows.push(window);
            }
        }
        windows.extend(active);
        Ok(windows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::TabSession;

    fn window(cwds: &[&str], active: usize) -> Session {
        Session::new(
            active,
            cwds.iter().map(|cwd| TabSession::single((*cwd).into(), None, None)).collect(),
        )
    }

    fn geometry(x: i32) -> WindowState {
        WindowState { x: Some(x), y: Some(80), width: 1200, height: 720, maximized: false }
    }

    #[test]
    fn combined_snapshot_exposes_active_geometry_for_ordinary_startup() {
        let mut first = window(&["/first"], 0);
        first.window = Some(geometry(100));
        let mut active = window(&["/active"], 0);
        active.window = Some(geometry(2100));
        let combined = combine_sessions([(false, first), (true, active.clone())]).unwrap();
        assert_eq!(combined.window, active.window);
    }

    #[test]
    fn empty_final_window_still_retains_its_geometry() {
        let mut empty = window(&[], 0);
        empty.window = Some(geometry(100));
        let combined = combine_sessions([(true, empty.clone())]).unwrap();
        assert!(combined.tabs.is_empty());
        assert!(!crate::session::should_restore(&combined));
        assert_eq!(combined.window, empty.window);
    }

    #[test]
    fn old_combined_snapshot_recovers_active_geometry_without_schema_change() {
        let mut first = window(&["/first"], 0);
        first.window = Some(geometry(100));
        let mut active = window(&["/active"], 0);
        active.window = Some(geometry(2100));
        let mut old = combine_sessions([(false, first), (true, active.clone())]).unwrap();
        old.window = None;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.json");
        crate::session::save_to(&path, &old).unwrap();
        let loaded = crate::session::load_from(&path).unwrap();
        assert_eq!(loaded.version, old.version);
        assert_eq!(loaded.tabs, old.tabs);
        assert_eq!(loaded.window, active.window);
    }

    #[test]
    fn explicit_top_level_geometry_is_authoritative_over_old_boundaries() {
        let mut original = window(&["/active"], 0);
        original.window = Some(geometry(100));
        let mut combined = combine_sessions([(true, original)]).unwrap();
        combined.window = Some(geometry(200));
        combined.recover_window_geometry();
        assert_eq!(combined.window, Some(geometry(200)));
    }

    #[test]
    fn snapshots_without_active_geometry_fall_back_to_the_first_saved_window() {
        let mut first = window(&["/first"], 0);
        first.window = Some(geometry(100));
        let mut combined =
            combine_sessions([(false, first.clone()), (true, window(&[], 0))]).unwrap();
        assert_eq!(combined.window, first.window);
        combined.window = None;
        combined.recover_window_geometry();
        assert_eq!(combined.window, first.window);
    }

    #[test]
    fn saved_workspace_retains_window_boundaries_and_active_tabs_for_update() {
        let first = window(&["/first", "/second"], 1);
        let second = window(&["/third"], 0);
        let combined = combine_sessions([(true, first.clone()), (false, second.clone())]).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.json");
        crate::session::save_to(&path, &combined).unwrap();
        let loaded = crate::session::load_from(&path).unwrap();
        assert_eq!(loaded.tabs.len(), 3, "the flat workspace remains compatible");
        assert_eq!(loaded.active_tab, 1);
        assert_eq!(loaded.into_update_windows().unwrap(), vec![second, first]);
    }

    #[test]
    fn moving_tabs_between_windows_changes_the_snapshot_even_if_flat_tabs_match() {
        let before = combine_sessions([
            (false, window(&["/first"], 0)),
            (true, window(&["/second", "/third"], 0)),
        ])
        .unwrap();
        let after = combine_sessions([
            (false, window(&["/first", "/second"], 0)),
            (true, window(&["/third"], 0)),
        ])
        .unwrap();
        assert_eq!(before.tabs, after.tabs);
        assert_ne!(before.window_layout, after.window_layout);
        assert_ne!(before, after, "autosave must persist window-only changes");
    }

    #[test]
    fn legacy_session_is_one_window_and_explicit_empty_session_stays_empty() {
        for legacy in [window(&["/old"], 0), window(&[], 0)] {
            assert_eq!(legacy.clone().into_update_windows().unwrap(), vec![legacy]);
        }
    }

    #[test]
    fn damaged_boundaries_cannot_silently_discard_tabs() {
        let original = combine_sessions([(true, window(&["/first"], 0))]).unwrap();
        for count in [0, 2, usize::MAX] {
            let mut damaged = original.clone();
            damaged.window_layout[0].tab_count = count;
            assert!(damaged.into_update_windows().is_err());
        }
        let mut damaged = original;
        damaged.window_layout[0].active_tab = 1;
        assert!(damaged.into_update_windows().is_err());
    }
}
