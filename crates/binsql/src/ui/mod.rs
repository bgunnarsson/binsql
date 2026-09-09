mod editor;
mod explorer;
mod header;
mod overlays;
pub(crate) mod results;
mod status;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
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
    frame.render_widget(Block::default().style(theme::body()), area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);

    let explorer_width = (rows[1].width * EXPLORER_PERCENT / 100)
        .clamp(EXPLORER_MIN.min(rows[1].width), EXPLORER_MAX);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(explorer_width), Constraint::Min(20)])
        .split(rows[1]);

    header::draw(frame, app, rows[0]);
    explorer::draw(frame, app, columns[0]);
    draw_workspace(frame, app, columns[1]);
    status::draw(frame, app, rows[2]);

    // Everything recedes behind an open modal, so the modal is plainly the
    // thing being talked to and the layout stays as context rather than as
    // competition.
    if app.overlay.is_some() {
        recede(frame, area, theme::SCRIM);
    }
    overlays::draw(frame, app, area);
}

/// Blends every cell in `area` toward the background.
///
/// Done to the finished buffer rather than by restyling each widget: the
/// alternative is every draw function taking a "dimmed" flag and remembering to
/// honour it, which is the sort of thing that is correct on the day it is
/// written and wrong a month later.
fn recede(frame: &mut Frame, area: Rect, amount: f32) {
    let buffer = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            cell.fg = theme::recede(cell.fg, amount);
            cell.bg = theme::recede(cell.bg, amount);
        }
    }
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

/// The bordered frame every pane and overlay shares.
///
/// binvim's popup form: the title sits in the top border after a single dash,
/// and a counter — rows, matches, position — sits at the right end of the same
/// border rather than competing with the content for a line.
pub fn pane(title: &str, focused: bool) -> Block<'static> {
    framed(title, None, focused, theme::chrome())
}

/// A pane over the body surface rather than the chrome one: the editor and the
/// result grid, which are content, not chrome.
pub fn body_pane(title: &str, counter: Option<String>, focused: bool) -> Block<'static> {
    framed(title, counter, focused, theme::body())
}

/// A pane with a counter at the right end of its top border.
pub fn counted_pane(title: &str, counter: impl Into<String>, focused: bool) -> Block<'static> {
    framed(title, Some(counter.into()), focused, theme::chrome())
}

fn framed(
    title: &str,
    counter: Option<String>,
    focused: bool,
    surface: ratatui::style::Style,
) -> Block<'static> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused))
        .title_top(Line::from(vec![
            Span::styled("─", theme::border(focused)),
            Span::styled(format!(" {title} "), theme::title(focused)),
        ]))
        .style(surface);

    if let Some(counter) = counter {
        block = block.title_top(
            Line::from(vec![
                Span::styled(format!(" {counter} "), theme::counter()),
                Span::styled("─", theme::border(focused)),
            ])
            .right_aligned(),
        );
    }
    block
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
