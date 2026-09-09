//! The Catppuccin Mocha palette, and the roles binsql assigns to it.
//!
//! Nothing outside this module names a colour. Widgets ask for a role — a
//! focused border, a NULL cell, a keyword — so the whole UI can be retinted by
//! editing one file.

use ratatui::style::{Color, Modifier, Style};

pub const BASE: Color = Color::Rgb(0x1e, 0x1e, 0x2e);
pub const MANTLE: Color = Color::Rgb(0x18, 0x18, 0x25);
pub const SURFACE0: Color = Color::Rgb(0x31, 0x32, 0x44);
pub const SURFACE1: Color = Color::Rgb(0x45, 0x47, 0x5a);
pub const SURFACE2: Color = Color::Rgb(0x58, 0x5b, 0x70);
pub const OVERLAY0: Color = Color::Rgb(0x6c, 0x70, 0x86);
pub const OVERLAY1: Color = Color::Rgb(0x7f, 0x84, 0x9c);
pub const SUBTEXT0: Color = Color::Rgb(0xa6, 0xad, 0xc8);
pub const SUBTEXT1: Color = Color::Rgb(0xba, 0xc2, 0xde);
pub const TEXT: Color = Color::Rgb(0xcd, 0xd6, 0xf4);
pub const BLUE: Color = Color::Rgb(0x89, 0xb4, 0xfa);
pub const LAVENDER: Color = Color::Rgb(0xb4, 0xbe, 0xfe);
pub const SAPPHIRE: Color = Color::Rgb(0x74, 0xc7, 0xec);
pub const TEAL: Color = Color::Rgb(0x94, 0xe2, 0xd5);
pub const GREEN: Color = Color::Rgb(0xa6, 0xe3, 0xa1);
pub const YELLOW: Color = Color::Rgb(0xf9, 0xe2, 0xaf);
pub const PEACH: Color = Color::Rgb(0xfa, 0xb3, 0x87);
pub const MAROON: Color = Color::Rgb(0xeb, 0xa0, 0xac);
pub const RED: Color = Color::Rgb(0xf3, 0x8b, 0xa8);
pub const MAUVE: Color = Color::Rgb(0xcb, 0xa6, 0xf7);

pub fn app() -> Style {
    Style::default().bg(BASE).fg(TEXT)
}

pub fn panel() -> Style {
    Style::default().bg(BASE).fg(TEXT)
}

pub fn border(focused: bool) -> Style {
    if focused {
        Style::default().fg(MAUVE)
    } else {
        Style::default().fg(SURFACE1)
    }
}

pub fn title(focused: bool) -> Style {
    if focused {
        Style::default().fg(MAUVE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY1)
    }
}

pub fn dim() -> Style {
    Style::default().fg(OVERLAY0)
}

pub fn muted() -> Style {
    Style::default().fg(SUBTEXT0)
}

pub fn accent() -> Style {
    Style::default().fg(BLUE)
}

pub fn success() -> Style {
    Style::default().fg(GREEN)
}

pub fn warning() -> Style {
    Style::default().fg(YELLOW)
}

pub fn danger() -> Style {
    Style::default().fg(RED)
}

/// The selected row in a pane that does not have focus. Kept visible but
/// quiet, so it is obvious which row you will return to.
pub fn selection(focused: bool) -> Style {
    if focused {
        Style::default()
            .bg(SURFACE1)
            .fg(TEXT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(SURFACE0).fg(SUBTEXT0)
    }
}

pub fn header() -> Style {
    Style::default()
        .bg(SURFACE0)
        .fg(LAVENDER)
        .add_modifier(Modifier::BOLD)
}

pub fn status_bar() -> Style {
    Style::default().bg(MANTLE).fg(SUBTEXT0)
}

pub fn tab(active: bool) -> Style {
    if active {
        Style::default()
            .bg(SURFACE0)
            .fg(TEXT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(MANTLE).fg(OVERLAY0)
    }
}

pub fn overlay() -> Style {
    Style::default().bg(MANTLE).fg(TEXT)
}

/// A key name in help text and the status bar.
pub fn key() -> Style {
    Style::default().fg(PEACH).add_modifier(Modifier::BOLD)
}

// --- Tree ---

pub fn node_source(connected: bool) -> Style {
    if connected {
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(SUBTEXT0)
    }
}

pub fn node_catalog(is_current: bool) -> Style {
    if is_current {
        Style::default().fg(SAPPHIRE)
    } else {
        Style::default().fg(SUBTEXT1)
    }
}

pub fn node_schema() -> Style {
    Style::default().fg(TEAL)
}

pub fn node_group() -> Style {
    Style::default().fg(OVERLAY1)
}

pub fn node_table() -> Style {
    Style::default().fg(TEXT)
}

pub fn node_view() -> Style {
    Style::default().fg(MAUVE)
}

pub fn node_column() -> Style {
    Style::default().fg(SUBTEXT0)
}

pub fn node_key_column() -> Style {
    Style::default().fg(YELLOW)
}

// --- Result grid ---

/// NULL is dimmed and italic so it never reads as the literal text "NULL"
/// stored in a column.
pub fn cell_null() -> Style {
    Style::default().fg(OVERLAY0).add_modifier(Modifier::ITALIC)
}

pub fn cell_number() -> Style {
    Style::default().fg(PEACH)
}

pub fn cell_bool() -> Style {
    Style::default().fg(MAROON)
}

pub fn cell_temporal() -> Style {
    Style::default().fg(TEAL)
}

pub fn cell_text() -> Style {
    Style::default().fg(TEXT)
}

// --- SQL syntax ---

pub fn sql_keyword() -> Style {
    Style::default().fg(MAUVE).add_modifier(Modifier::BOLD)
}

pub fn sql_text() -> Style {
    Style::default().fg(TEXT)
}
