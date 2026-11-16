//! Bounded, presentation-only label recognition. No terminal or Agent state changes.

pub(super) const PREFIX_COLS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LabelKind {
    Info,
    Warn,
    Error,
    Tool,
    Failure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Label {
    pub start: u16,
    pub end: u16,
    pub kind: LabelKind,
}

impl Label {
    pub fn contains(self, col: u16) -> bool {
        (self.start..self.end).contains(&col)
    }

    pub fn drawable(self, columns: u16, blocked: impl Fn(u16) -> bool) -> bool {
        self.start < self.end && self.end <= columns && !(self.start..self.end).any(blocked)
    }

    pub fn overlaps(self, start: u16, end: u16) -> bool {
        self.start < self.end && start < end && self.start < end && start < self.end
    }
}

pub(super) fn classify_prefix(prefix: &[char], provider: Option<&str>) -> Option<Label> {
    if provider == Some("claude") && prefix.starts_with(&['⏺', ' ']) {
        for name in [
            "Bash",
            "Read",
            "Write",
            "Edit",
            "Update",
            "MultiEdit",
            "Glob",
            "Grep",
            "WebFetch",
            "WebSearch",
            "Task",
            "Agent",
            "TodoWrite",
            "NotebookEdit",
            "Skill",
            "ToolSearch",
        ] {
            let end = 2 + name.len();
            if starts_with(&prefix[2..], name)
                && prefix.get(end) == Some(&'(')
                && prefix.get(end + 1).is_some_and(|ch| !ch.is_whitespace())
            {
                return Some(Label { start: 2, end: end as u16, kind: LabelKind::Tool });
            }
        }
        return None;
    }
    let timestamp_end = timestamp_end(prefix);
    let start = timestamp_end.unwrap_or(0);
    let bracketed = prefix.get(start) == Some(&'[');
    let word_start = start + usize::from(bracketed);
    let tail = prefix.get(word_start..)?;
    for (name, kind) in
        [("INFO", LabelKind::Info), ("WARN", LabelKind::Warn), ("ERROR", LabelKind::Error)]
    {
        if !starts_with(tail, name) {
            continue;
        }
        let end = word_start + name.len();
        let mut message = end;
        if bracketed {
            if prefix.get(message) != Some(&']') {
                return None;
            }
            message += 1;
        } else if prefix.get(message) == Some(&':') {
            message += 1;
        } else if timestamp_end.is_none() && prefix.get(message..message + 2) != Some(&[' ', ' ']) {
            return None;
        }
        if prefix.get(message) != Some(&' ') {
            return None;
        }
        while prefix.get(message) == Some(&' ') {
            message += 1;
        }
        // Assignment-shaped code and missing/clipped messages are not log entries.
        if prefix.get(message).is_none_or(|ch| ch.is_whitespace() || matches!(ch, '=' | ':' | '['))
        {
            return None;
        }
        return Some(Label { start: word_start as u16, end: end as u16, kind });
    }
    None
}

#[derive(Debug)]
pub(super) struct BadgeGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}

pub(super) fn badge_geometry(
    cell_width: f32,
    line_height: f32,
    start: u16,
    end: u16,
) -> Option<BadgeGeometry> {
    if cell_width <= 0.0
        || line_height <= 0.0
        || !cell_width.is_finite()
        || !line_height.is_finite()
        || start >= end
    {
        return None;
    }
    let height = line_height * 0.88;
    let width = cell_width * f32::from(end - start);
    Some(BadgeGeometry {
        x: cell_width * f32::from(start),
        y: line_height * 0.06,
        width,
        height,
        radius: (height * 0.5).min(width * 0.5),
    })
}

fn starts_with(prefix: &[char], text: &str) -> bool {
    prefix.len() >= text.len() && prefix.iter().copied().take(text.len()).eq(text.chars())
}

fn decimal(chars: &[char]) -> Option<u32> {
    chars.iter().try_fold(0, |value, ch| ch.to_digit(10).map(|digit| value * 10 + digit))
}

/// Explicit HH:MM:SS[.fraction] or ISO date + time, optionally bracketed.
/// Numeric ranges are validated so an arbitrary numeric prefix cannot opt in.
fn timestamp_end(prefix: &[char]) -> Option<usize> {
    let bracketed = prefix.first() == Some(&'[');
    let mut at = usize::from(bracketed);
    if prefix.get(at + 4) == Some(&'-') {
        decimal(prefix.get(at..at + 4)?)?;
        if prefix.get(at + 7) != Some(&'-') {
            return None;
        }
        let month = decimal(prefix.get(at + 5..at + 7)?)?;
        let day = decimal(prefix.get(at + 8..at + 10)?)?;
        if !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || !matches!(prefix.get(at + 10), Some('T' | ' '))
        {
            return None;
        }
        at += 11;
    }
    if prefix.get(at + 2) != Some(&':') || prefix.get(at + 5) != Some(&':') {
        return None;
    }
    let hour = decimal(prefix.get(at..at + 2)?)?;
    let minute = decimal(prefix.get(at + 3..at + 5)?)?;
    let second = decimal(prefix.get(at + 6..at + 8)?)?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    at += 8;
    if prefix.get(at) == Some(&'.') {
        at += 1;
        let fraction_start = at;
        while prefix.get(at).is_some_and(char::is_ascii_digit) {
            at += 1;
        }
        if at == fraction_start {
            return None;
        }
    }
    if prefix.get(at) == Some(&'Z') {
        at += 1;
    }
    if bracketed {
        if prefix.get(at) != Some(&']') {
            return None;
        }
        at += 1;
    }
    if prefix.get(at) != Some(&' ') {
        return None;
    }
    while prefix.get(at) == Some(&' ') {
        at += 1;
    }
    Some(at)
}

#[cfg(test)]
#[path = "label_badges_classifier_tests.rs"]
mod tests;
