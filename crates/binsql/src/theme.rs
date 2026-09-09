//! The palette, and the roles binsql assigns to it.
//!
//! The vocabulary is binvim's, deliberately: the same twelve chrome roles, the
//! same Catppuccin Mocha values behind them, and the same rule that chrome sits
//! on its own surface so it reads as layered above the body rather than painted
//! into it. Anything here that looks like binvim is meant to.
//!
//! Nothing outside this module names a colour. Widgets ask for a role — a
//! focused border, a NULL cell, a selected row — so the whole UI can be
//! retinted by editing one file.

use ratatui::style::{Color, Modifier, Style};

// ── Surfaces ────────────────────────────────────────────────────────
/// The body: the query buffer and the result grid.
pub const BACKGROUND: Color = Color::Rgb(0x1e, 0x1e, 0x2e);
/// Chrome: side pane, tab strip, status line, every overlay. Always a shade
/// off `BACKGROUND` so chrome reads as sitting above the body.
pub const CHROME_BG: Color = Color::Rgb(0x18, 0x18, 0x25);

// ── Chrome palette — binvim's twelve keys ───────────────────────────
/// Main text on chrome.
pub const FOREGROUND: Color = Color::Rgb(0xcd, 0xd6, 0xf4);
/// Muted text: hints, types, row numbers.
pub const DIM: Color = Color::Rgb(0x6c, 0x70, 0x86);
/// Active tab, overlay title, the selection bar.
pub const EMPHASIS: Color = Color::Rgb(0xb4, 0xbe, 0xfe);
/// Layered chrome: selection background, active tab background.
pub const SURFACE: Color = Color::Rgb(0x45, 0x47, 0x5a);
/// Borders and dividers.
pub const BORDER: Color = Color::Rgb(0x58, 0x5b, 0x70);
/// The prompt caret, the focused-pane chip.
pub const ACCENT: Color = Color::Rgb(0xfa, 0xb3, 0x87);
/// A connected data source, a successful run.
pub const ACCENT_SECONDARY: Color = Color::Rgb(0xa6, 0xe3, 0xa1);
/// Foreground on a bright chip.
pub const CHIP_FG: Color = Color::Rgb(0x1e, 0x1e, 0x2e);
pub const ERROR: Color = Color::Rgb(0xf3, 0x8b, 0xa8);
pub const WARNING: Color = Color::Rgb(0xf9, 0xe2, 0xaf);
pub const INFO: Color = Color::Rgb(0x89, 0xb4, 0xfa);
pub const HINT: Color = Color::Rgb(0x89, 0xdc, 0xeb);

// A few values the grid and tree colour by, beyond the twelve.
const SUBTEXT: Color = Color::Rgb(0xa6, 0xad, 0xc8);
const MAUVE: Color = Color::Rgb(0xcb, 0xa6, 0xf7);
const TEAL: Color = Color::Rgb(0x94, 0xe2, 0xd5);
const MAROON: Color = Color::Rgb(0xeb, 0xa0, 0xac);
/// One step above `SURFACE`, for the cell directly under the cursor.
const SURFACE_HIGH: Color = Color::Rgb(0x58, 0x5b, 0x70);
/// One step below, for the rest of the cursor's row.
const SURFACE_LOW: Color = Color::Rgb(0x31, 0x32, 0x44);

// ── Powerline ───────────────────────────────────────────────────────
// Nerd Font glyphs, used directly: binvim assumes a Nerd Font in the terminal
// and binsql sits beside it in the same one.
pub const PL_RIGHT: char = '\u{e0b0}';
pub const PL_LEFT: char = '\u{e0b2}';

// ── Icons ───────────────────────────────────────────────────────────
pub const ICON_SERVER: char = '\u{f233}';
pub const ICON_DATABASE: char = '\u{f1c0}';
pub const ICON_SCHEMA: char = '\u{f07b}';
pub const ICON_TABLE: char = '\u{f0ce}';
pub const ICON_VIEW: char = '\u{f06e}';
pub const ICON_COLUMN: char = '\u{f0db}';
pub const ICON_KEY: char = '\u{f084}';
/// The bar down the left of a selected row.
pub const SELECTION_BAR: char = '▌';

pub fn body() -> Style {
    Style::default().bg(BACKGROUND).fg(FOREGROUND)
}

pub fn chrome() -> Style {
    Style::default().bg(CHROME_BG).fg(FOREGROUND)
}

pub fn border(focused: bool) -> Style {
    Style::default().fg(if focused { EMPHASIS } else { BORDER })
}

/// A pane or overlay title, embedded in its top border.
pub fn title(focused: bool) -> Style {
    if focused {
        Style::default().fg(EMPHASIS).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DIM)
    }
}

/// The counter binvim puts at the right end of a popup's top border.
pub fn counter() -> Style {
    Style::default().fg(DIM)
}

pub fn dim() -> Style {
    Style::default().fg(DIM)
}

pub fn muted() -> Style {
    Style::default().fg(SUBTEXT)
}

pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

pub fn success() -> Style {
    Style::default().fg(ACCENT_SECONDARY)
}

pub fn warning() -> Style {
    Style::default().fg(WARNING)
}

pub fn danger() -> Style {
    Style::default().fg(ERROR)
}

/// A bright chip: the pane name in the status line.
pub fn chip(colour: Color) -> Style {
    Style::default()
        .bg(colour)
        .fg(CHIP_FG)
        .add_modifier(Modifier::BOLD)
}

/// The arrow between two powerline segments: the outgoing segment's colour
/// drawn over the incoming one's.
pub fn powerline(from: Color, to: Color) -> Style {
    Style::default().fg(from).bg(to)
}

/// The background of a selected row. The `▌` bar carries the emphasis; the
/// background only says which row.
pub fn selection(focused: bool) -> Style {
    Style::default().bg(if focused { SURFACE } else { SURFACE_LOW })
}

pub fn selection_bar(focused: bool) -> Style {
    Style::default()
        .fg(if focused { EMPHASIS } else { BORDER })
        .bg(if focused { SURFACE } else { SURFACE_LOW })
}

pub fn header() -> Style {
    Style::default()
        .bg(SURFACE)
        .fg(EMPHASIS)
        .add_modifier(Modifier::BOLD)
}

pub fn status_bar() -> Style {
    Style::default().bg(CHROME_BG).fg(SUBTEXT)
}

pub fn tab(active: bool) -> Style {
    if active {
        Style::default()
            .bg(SURFACE)
            .fg(EMPHASIS)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(CHROME_BG).fg(DIM)
    }
}

pub fn overlay() -> Style {
    Style::default().bg(CHROME_BG).fg(FOREGROUND)
}

/// A key name in help text and the status bar.
pub fn key() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

// ── Tree ────────────────────────────────────────────────────────────

pub fn node_source(connected: bool) -> Style {
    if connected {
        Style::default().fg(FOREGROUND).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(SUBTEXT)
    }
}

pub fn node_catalog(is_current: bool) -> Style {
    Style::default().fg(if is_current { HINT } else { SUBTEXT })
}

pub fn node_schema() -> Style {
    Style::default().fg(TEAL)
}

pub fn node_group() -> Style {
    Style::default().fg(BORDER)
}

pub fn node_table() -> Style {
    Style::default().fg(FOREGROUND)
}

pub fn node_view() -> Style {
    Style::default().fg(MAUVE)
}

pub fn node_column() -> Style {
    Style::default().fg(SUBTEXT)
}

pub fn node_key_column() -> Style {
    Style::default().fg(WARNING)
}

// ── Result grid ─────────────────────────────────────────────────────

/// NULL is dimmed and italic so it never reads as the literal text "NULL"
/// stored in a column.
pub fn cell_null() -> Style {
    Style::default().fg(DIM).add_modifier(Modifier::ITALIC)
}

pub fn cell_number() -> Style {
    Style::default().fg(ACCENT)
}

pub fn cell_bool() -> Style {
    Style::default().fg(MAROON)
}

pub fn cell_temporal() -> Style {
    Style::default().fg(TEAL)
}

pub fn cell_text() -> Style {
    Style::default().fg(FOREGROUND)
}

/// The cell under the grid cursor.
pub fn cell_cursor(focused: bool) -> Color {
    if focused { SURFACE_HIGH } else { SURFACE }
}

/// The rest of the cursor's row.
pub fn row_cursor() -> Color {
    SURFACE_LOW
}

// ── SQL ─────────────────────────────────────────────────────────────

pub fn sql_keyword() -> Style {
    Style::default().fg(MAUVE).add_modifier(Modifier::BOLD)
}

pub fn sql_text() -> Style {
    Style::default().fg(FOREGROUND)
}
