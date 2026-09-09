mod editor;
mod explorer;
mod overlays;
pub(crate) mod results;
mod status;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, BorderType, Borders};

use crate::app::App;
use crate::theme;

/// The explorer's share of the width, bounded so it stays usable in a narrow
/// terminal and does not swallow a wide one.
const EXPLORER_PERCENT: u16 = 26;
const EXPLORER_MIN: u16 = 24;
const EXPLORER_MAX: u16 = 46;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(theme::app()), area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let explorer_width = (rows[0].width * EXPLORER_PERCENT / 100)
        .clamp(EXPLORER_MIN.min(rows[0].width), EXPLORER_MAX);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(explorer_width), Constraint::Min(20)])
        .split(rows[0]);

    explorer::draw(frame, app, columns[0]);
    draw_workspace(frame, app, columns[1]);
    status::draw(frame, app, rows[1]);

    overlays::draw(frame, app, area);
}

fn draw_workspace(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Percentage(38),
            Constraint::Min(5),
        ])
        .split(area);

    editor::draw_tabs(frame, app, rows[0]);
    editor::draw(frame, app, rows[1]);
    results::draw(frame, app, rows[2]);
}

/// The bordered frame every pane shares, so focus reads the same everywhere.
pub fn pane(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused))
        .title(format!(" {title} "))
        .title_style(theme::title(focused))
        .style(theme::panel())
}

/// Centres a box of the given size inside `area`, clamped to fit.
pub fn centered(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let width = (area.width * width_percent / 100).min(area.width);
    let height = (area.height * height_percent / 100).min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

/// Centres a box of an explicit size, clamped to fit `area`.
///
/// For overlays whose height follows their content: a record with four fields
/// should not be given the same modal as one with forty.
pub fn centered_size(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// Cuts a string to `width` display columns, marking that it was cut.
pub fn truncate(text: &str, width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".to_string();
    }

    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width - 1 {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}

/// Keeps `selected` inside a window of `height` rows, adjusting `offset` by the
/// least amount that works.
pub fn scroll_offset(offset: usize, selected: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    if selected < offset {
        selected
    } else if selected >= offset + height {
        selected + 1 - height
    } else {
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_marks_what_it_cut() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello w…");
        assert_eq!(truncate("hello", 1), "…");
    }

    #[test]
    fn scrolling_moves_the_minimum() {
        assert_eq!(scroll_offset(0, 5, 10), 0);
        assert_eq!(scroll_offset(0, 12, 10), 3);
        assert_eq!(scroll_offset(10, 4, 10), 4);
    }
}
