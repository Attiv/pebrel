//! Semantic prompt marks and input boundaries owned by the terminal grid.
use std::collections::VecDeque;

use super::{Term, TermMode};
use crate::event::EventListener;
use crate::grid::{Dimensions, Scroll};
use crate::index::{Column, Line, Point};
use crate::vte::ansi::KeyboardModes;

/// Shell-owned state is distinct from transient application negotiation.
#[derive(Default)]
pub(super) struct PromptState {
    /// Strictly increasing absolute primary-screen prompt rows.
    marks: VecDeque<usize>,
    active: bool,
    input: Option<(usize, Column)>,
    baseline: Option<InputModeSnapshot>,
    command_running: bool,
    generation: u64,
    output_revision: u64,
    recovered_alt_shell: bool,
}

impl PromptState {
    pub(super) fn clear_geometry(&mut self) {
        self.marks.clear();
        self.input = None;
    }

    pub(super) fn reset(&mut self) {
        *self = Self {
            generation: self.generation.wrapping_add(1),
            output_revision: self.output_revision,
            ..Self::default()
        };
    }

    pub(super) fn screen_swapped(&mut self) {
        self.recovered_alt_shell = false;
    }
}

/// Opaque ownership/output snapshot for a verified, potentially async shell
/// return. A newer command, output chunk or reset invalidates the snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputModeLease {
    generation: u64,
    output_revision: u64,
}

struct InputModeSnapshot {
    mode: TermMode,
    keyboard_stack: Vec<KeyboardModes>,
    inactive_keyboard_stack: Vec<KeyboardModes>,
}

// Restore only input contracts, not screen contents, cursor, layout or colors.
const INPUT_MODES: TermMode = TermMode::APP_CURSOR
    .union(TermMode::APP_KEYPAD)
    .union(TermMode::MOUSE_MODE)
    .union(TermMode::SGR_MOUSE)
    .union(TermMode::UTF8_MOUSE)
    .union(TermMode::FOCUS_IN_OUT)
    .union(TermMode::BRACKETED_PASTE)
    .union(TermMode::KITTY_KEYBOARD_PROTOCOL)
    .union(TermMode::WIN32_INPUT_MODE);

impl<T> Term<T> {
    fn capture_prompt_input_modes(&mut self) {
        self.prompt.baseline = Some(InputModeSnapshot {
            mode: self.mode & INPUT_MODES,
            keyboard_stack: self.keyboard_mode_stack.clone(),
            inactive_keyboard_stack: self.inactive_keyboard_mode_stack.clone(),
        });
    }

    fn restore_prompt_input_modes(&mut self) {
        if let Some(mut baseline) = self.prompt.baseline.take() {
            if !self.config.kitty_keyboard {
                baseline.mode.remove(TermMode::KITTY_KEYBOARD_PROTOCOL);
                baseline.keyboard_stack.clear();
                baseline.inactive_keyboard_stack.clear();
            }
            self.mode = (self.mode - INPUT_MODES) | baseline.mode;
            // An exited owner may leave its alternate buffer visible. Keep the
            // shell's contract on both buffers so a late 1049l cannot restore
            // that owner's keyboard flags, without discarding prompt/cursor.
            self.inactive_keyboard_mode_stack = if self.mode.contains(TermMode::ALT_SCREEN) {
                baseline.keyboard_stack.clone()
            } else {
                baseline.inactive_keyboard_stack
            };
            self.keyboard_mode_stack = baseline.keyboard_stack;
        }
        self.prompt.command_running = false;
    }

    /// Capture the shell's negotiation before a primary-screen command starts.
    /// Repeated command markers must not adopt the application's leaked modes.
    pub fn nebula_start_command(&mut self) {
        if self.nebula_shell_input_screen() && !self.prompt.command_running {
            self.nebula_begin_shell_input();
        }
        self.nebula_end_prompt();
    }

    /// UI adapters may call this only after confirming a real shell submission,
    /// before sending Enter. Foreground TUI input must not begin another lease.
    pub fn nebula_begin_shell_input(&mut self) -> Option<InputModeLease> {
        if !self.nebula_shell_input_screen() {
            return None;
        }
        self.capture_prompt_input_modes();
        self.prompt.generation = self.prompt.generation.wrapping_add(1);
        self.prompt.command_running = true;
        self.prompt.recovered_alt_shell = false;
        self.nebula_end_prompt();
        self.nebula_input_mode_lease()
    }

    /// A verified shell may still be drawn in an abandoned alternate buffer.
    /// This is not permission to treat input inside a live TUI as shell input.
    pub fn nebula_shell_input_screen(&self) -> bool {
        !self.mode.contains(TermMode::ALT_SCREEN) || self.prompt.recovered_alt_shell
    }

    pub fn nebula_input_mode_lease(&self) -> Option<InputModeLease> {
        (self.prompt.command_running && self.prompt.baseline.is_some()).then_some(InputModeLease {
            generation: self.prompt.generation,
            output_revision: self.prompt.output_revision,
        })
    }

    pub fn nebula_owns_input_modes(&self, lease: InputModeLease) -> bool {
        self.nebula_input_mode_lease().is_some_and(|current| current.generation == lease.generation)
    }

    /// StreamProcessor calls this once per nonempty feed, before parsing it.
    pub(crate) fn nebula_note_output(&mut self) {
        self.prompt.output_revision = self.prompt.output_revision.wrapping_add(1);
    }

    /// The caller must verify shell return/owner exit at this exact output
    /// snapshot. Unlike a raw OSC inside a TUI, that can recover abandoned ALT.
    pub fn nebula_restore_input_modes(&mut self, lease: InputModeLease) -> bool {
        if self.nebula_input_mode_lease() != Some(lease) {
            return false;
        }
        self.restore_prompt_input_modes();
        self.prompt.recovered_alt_shell = self.mode.contains(TermMode::ALT_SCREEN);
        true
    }

    /// Recover synchronously at a shell boundary, not an async UI/Agent event.
    /// Alternate-screen embedded shells are still owned by the live TUI.
    pub fn nebula_finish_command(&mut self) {
        if !self.mode.contains(TermMode::ALT_SCREEN) {
            self.restore_prompt_input_modes();
        }
        self.nebula_end_prompt();
    }

    /// The cursor row in the grid's absolute line numbering (see
    /// [`Grid::scrolled_out`]): stable across scrollback growth, so overlays
    /// (prompt marks, inline images) can anchor to it.
    pub fn nebula_cursor_abs_line(&self) -> usize {
        self.grid.scrolled_out()
            + self.grid.history_size()
            + self.grid.cursor.point.line.0.max(0) as usize
    }

    /// Record a shell prompt row (OSC 133;A) at the current cursor line.
    ///
    /// Called by the PTY reader between `parser.advance` slices, so the cursor
    /// sits exactly on the fresh prompt row. Marks only make sense on the
    /// primary screen — the alternate screen has no scrollback to jump.
    pub fn nebula_add_prompt_mark(&mut self) {
        if self.mode.contains(TermMode::ALT_SCREEN) {
            return;
        }
        // A new prompt is the fallback boundary when no command-done marker
        // was emitted. Do not reset a shell's own active prompt redraw.
        if !self.prompt.active {
            self.restore_prompt_input_modes();
        }
        self.capture_prompt_input_modes();
        let abs = self.nebula_cursor_abs_line();

        // A screen redraw (clear, resize) can re-emit a mark for the same or
        // an earlier row; drop those so the deque stays strictly increasing.
        while self.prompt.marks.back().is_some_and(|&m| m >= abs) {
            self.prompt.marks.pop_back();
        }
        // Prune marks whose rows have scrolled out of history entirely.
        let floor = self.grid.scrolled_out();
        while self.prompt.marks.front().is_some_and(|&m| m < floor) {
            self.prompt.marks.pop_front();
        }

        self.prompt.marks.push_back(abs);
        self.prompt.active = true;
        self.prompt.input = None;
    }

    pub fn nebula_end_prompt(&mut self) {
        self.prompt.active = false;
        self.prompt.input = None;
    }

    pub fn nebula_prompt_active(&self) -> bool {
        self.prompt.active && !self.mode.contains(TermMode::ALT_SCREEN)
    }

    /// Capture OSC 133;B between parser slices, before input is echoed.
    pub fn nebula_mark_prompt_input(&mut self) {
        if self.nebula_prompt_active() {
            let mut line = self.nebula_cursor_abs_line();
            let mut column = self.grid.cursor.point.column;
            if self.grid.cursor.input_needs_wrap {
                line += 1;
                column = Column(0);
            }
            self.prompt.input = Some((line, column));
        }
    }

    /// Resolve the boundary after scrollback growth; reflow/reset discard it.
    /// A column equal to the grid width denotes an empty pending-wrap boundary.
    pub fn nebula_prompt_input_point(&self) -> Option<Point> {
        if !self.nebula_prompt_active() {
            return None;
        }
        let (line, column) = self.prompt.input?;
        if line < self.grid.scrolled_out() || column.0 >= self.columns() {
            return None;
        }
        // Until input is echoed, a full-width prompt's next insertion point is
        // still on the filled row. This also covers the bottom row, where the
        // eventual wrap will scroll the grid before displaying the input.
        if self.grid.cursor.input_needs_wrap
            && line == self.nebula_cursor_abs_line() + 1
            && column == Column(0)
        {
            return Some(Point::new(self.grid.cursor.point.line, Column(self.columns())));
        }
        let relative =
            line as i64 - self.grid.scrolled_out() as i64 - self.grid.history_size() as i64;
        let point = Point::new(Line(i32::try_from(relative).ok()?), column);
        (point.line >= self.grid.topmost_line() && point.line <= self.grid.bottommost_line())
            .then_some(point)
    }

    /// Scroll the viewport to the previous (`up`) or next shell prompt mark.
    ///
    /// Returns whether the viewport moved, so callers know to redraw.
    pub fn nebula_prompt_jump(&mut self, up: bool) -> bool
    where
        T: EventListener,
    {
        if self.mode.contains(TermMode::ALT_SCREEN) || self.prompt.marks.is_empty() {
            return false;
        }

        let scrolled_out = self.grid.scrolled_out();
        let history = self.grid.history_size();
        // Absolute line currently shown at the top of the viewport.
        let top_abs = scrolled_out + history - self.grid.display_offset();

        let target = if up {
            self.prompt.marks.iter().rev().find(|&&m| m < top_abs)
        } else {
            self.prompt.marks.iter().find(|&&m| m > top_abs)
        };
        let Some(&mark) = target else { return false };

        // Put the mark's row at the viewport top: offset = history - relative
        // row. Marks on the visible screen clamp to 0 (bottom), long-gone
        // marks clamp to the scrollback top.
        let offset = (scrolled_out + history).saturating_sub(mark).min(history);
        let delta = offset as i32 - self.grid.display_offset() as i32;
        if delta == 0 {
            return false;
        }
        self.scroll_display(Scroll::Delta(delta));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::VoidListener;
    use crate::event_loop::StreamProcessor;
    use crate::term::{Config, test::TermSize};

    fn terminal() -> Term<VoidListener> {
        Term::new(
            Config { kitty_keyboard: true, ..Default::default() },
            &TermSize::new(80, 24),
            VoidListener,
        )
    }

    #[test]
    fn input_recovery_lease_rejects_new_output_commands_reset_and_completed_owners() {
        let mut term = terminal();
        let mut stream = StreamProcessor::default();
        let initial = term.nebula_begin_shell_input().unwrap();
        stream.feed(&mut term, &VoidListener, b"\x1b[>31u\x1b[?1000h");
        assert!(!term.nebula_restore_input_modes(initial));
        let current = term.nebula_input_mode_lease().unwrap();
        stream.feed(&mut term, &VoidListener, b"");
        assert_eq!(term.nebula_input_mode_lease(), Some(current));
        assert!(term.nebula_restore_input_modes(current));
        assert!(!term.nebula_restore_input_modes(current));
        let next = term.nebula_begin_shell_input().unwrap();
        assert!(!term.nebula_owns_input_modes(current));
        stream.feed(&mut term, &VoidListener, b"\x1bc");
        let after_reset = term.nebula_begin_shell_input().unwrap();
        assert!(!term.nebula_restore_input_modes(next));
        stream.feed(&mut term, &VoidListener, b"\x1b]133;D;0\x07\x1b]133;C\x07\x1b[>31u");
        assert!(!term.nebula_restore_input_modes(after_reset));
        assert!(term.mode().contains(TermMode::REPORT_ALL_KEYS_AS_ESC));
    }

    #[test]
    fn verified_owner_exit_recovers_abandoned_alt_and_respects_current_config() {
        let mut term = terminal();
        let mut stream = StreamProcessor::default();
        stream.feed(&mut term, &VoidListener, b"\x1b[=1u\x1b[?9001h\x1b[?2004h");
        term.nebula_begin_shell_input();
        stream.feed(
            &mut term,
            &VoidListener,
            b"\x1b[>31u\x1b[?1049h\x1b[>31u\x1b[?1000h\x1b[?1006h",
        );
        term.set_options(Config::default());
        let lease = term.nebula_input_mode_lease().unwrap();
        assert!(term.nebula_restore_input_modes(lease));
        assert!(!term.mode().intersects(TermMode::KITTY_KEYBOARD_PROTOCOL | TermMode::MOUSE_MODE));
        assert!(term.mode().contains(TermMode::ALT_SCREEN));
        assert!(term.mode().contains(TermMode::WIN32_INPUT_MODE | TermMode::BRACKETED_PASTE));
        stream.feed(&mut term, &VoidListener, b"\x1b[?1049h\x1b[?1049l");
        assert!(!term.mode().intersects(TermMode::KITTY_KEYBOARD_PROTOCOL));
    }

    #[test]
    fn repeated_shell_commands_can_recover_on_a_preserved_abandoned_alt_buffer() {
        let mut term = terminal();
        let mut stream = StreamProcessor::default();
        term.nebula_begin_shell_input().unwrap();
        stream.feed(&mut term, &VoidListener, b"\x1b[?1049h\x1b[>31u\x1b[?1000h");
        assert!(term.nebula_restore_input_modes(term.nebula_input_mode_lease().unwrap()));
        assert!(term.mode().contains(TermMode::ALT_SCREEN));
        let next = term
            .nebula_begin_shell_input()
            .expect("verified shell may submit on its preserved buffer");
        stream.feed(&mut term, &VoidListener, b"\x1b[>31u\x1b[?1000h");
        assert!(!term.nebula_restore_input_modes(next));
        assert!(term.nebula_restore_input_modes(term.nebula_input_mode_lease().unwrap()));
        assert!(!term.mode().intersects(TermMode::KITTY_KEYBOARD_PROTOCOL | TermMode::MOUSE_MODE));
    }

    #[test]
    #[ignore = "manual local cost evidence, no timing threshold"]
    fn input_recovery_stream_cost_probe() {
        let mut term = terminal();
        let mut stream = StreamProcessor::default();
        let started = std::time::Instant::now();
        let chunk = [b'x'; 256];
        for _ in 0..10_000 {
            stream.feed(&mut term, &VoidListener, &chunk);
        }
        eprintln!("plain 10000x256-byte feeds: {:?}", started.elapsed());
        let started = std::time::Instant::now();
        for _ in 0..10_000 {
            stream.feed(
                &mut term,
                &VoidListener,
                b"\x1b]133;A\x07\x1b]133;C\x07\x1b[>31u\x1b[?1000h\x1b]133;D;0\x07",
            );
        }
        eprintln!("10000 command boundaries, empty shell stack: {:?}", started.elapsed());
        for _ in 0..super::super::KEYBOARD_MODE_STACK_MAX_DEPTH {
            stream.feed(&mut term, &VoidListener, b"\x1b[>1u");
        }
        let started = std::time::Instant::now();
        for _ in 0..1_000 {
            stream.feed(&mut term, &VoidListener, b"\x1b]133;A\x07\x1b]133;C\x07\x1b]133;D;0\x07");
        }
        eprintln!("1000 boundaries, capped 4096-frame shell stack: {:?}", started.elapsed());
    }
}
