use binsql_core::ObjectKind;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::tree::{LoadState, NodeKind};
use crate::app::{App, Pane};
use crate::theme;
use crate::ui;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Pane::Explorer;
    let connected = app.sessions.len();
    let total = app.config.len();
    let block = ui::counted_pane("Databases", format!("{connected}/{total}"), focused);
    let inner = block.inner(area);
    app.panes.tree = inner;
    frame.render_widget(block, area);

    if app.config.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("No data sources yet.", theme::muted())),
                Line::from(""),
                Line::from(vec![
                    Span::styled("n", theme::key()),
                    Span::styled(" adds one", theme::dim()),
                ]),
            ]),
            inner,
        );
        return;
    }

    let visible = app.tree.visible();
    let height = inner.height as usize;
    app.tree.offset = ui::scroll_offset(app.tree.offset, app.tree.selected, height);

    let lines: Vec<Line> = visible
        .iter()
        .enumerate()
        .skip(app.tree.offset)
        .take(height)
        .map(|(index, entry)| {
            let selected = index == app.tree.selected;
            render_node(app, entry.id, entry.depth, selected, inner.width)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_node(
    app: &App,
    id: crate::app::tree::NodeId,
    depth: usize,
    selected: bool,
    width: u16,
) -> Line<'static> {
    let Some(node) = app.tree.find(id) else {
        return Line::default();
    };

    // Column zero belongs to the selection bar on every row, selected or not,
    // so marking a row never shifts its contents sideways.
    let mut spans = vec![
        if selected {
            Span::styled(
                theme::SELECTION_BAR.to_string(),
                theme::selection_bar(app.focus == Pane::Explorer),
            )
        } else {
            Span::raw(" ")
        },
        Span::raw("  ".repeat(depth)),
    ];

    let expander = match (&node.state, node.expanded) {
        (LoadState::Loading, _) => "◐ ",
        (LoadState::Leaf, _) => "  ",
        (_, true) => "▾ ",
        (_, false) => "▸ ",
    };
    spans.push(Span::styled(expander, theme::dim()));

    match &node.kind {
        NodeKind::Folder { name } => {
            spans.push(Span::styled(
                format!("{} ", theme::ICON_FOLDER),
                theme::node_folder(),
            ));
            spans.push(Span::styled(name.clone(), theme::node_folder()));
            spans.push(Span::styled(
                format!("  {}", node.children.len()),
                theme::dim(),
            ));
        }

        NodeKind::Source { name, connected } => {
            let glyph_style = if *connected {
                theme::success()
            } else if node.state == LoadState::Loading {
                theme::warning()
            } else {
                theme::dim()
            };
            spans.push(Span::styled(
                format!("{} ", theme::ICON_SERVER),
                glyph_style,
            ));
            let leaf = binsql_core::config::split_qualified(name).1;
            spans.push(Span::styled(
                leaf.to_string(),
                theme::node_source(*connected),
            ));

            if let Some(source) = app.config.get(name) {
                spans.push(Span::styled(
                    format!("  {}", source.backend.label()),
                    theme::dim(),
                ));
                if source.read_only {
                    spans.push(Span::styled("  ro", theme::warning()));
                }
            }
        }

        NodeKind::Catalog { name, is_current } => {
            let style = theme::node_catalog(*is_current);
            spans.push(Span::styled(format!("{} ", theme::ICON_DATABASE), style));
            spans.push(Span::styled(name.clone(), style));
            if *is_current {
                spans.push(Span::styled("  ·", theme::dim()));
            }
        }

        NodeKind::Schema { name } => {
            spans.push(Span::styled(
                format!("{} ", theme::ICON_SCHEMA),
                theme::node_schema(),
            ));
            spans.push(Span::styled(name.clone(), theme::node_schema()));
        }

        NodeKind::Group { kind } => {
            spans.push(Span::styled(kind.label().to_string(), theme::node_group()));
            spans.push(Span::styled(
                format!("  {}", node.children.len()),
                theme::dim(),
            ));
        }

        NodeKind::Object { object } => {
            let (icon, style) = match object.kind {
                ObjectKind::Table => (theme::ICON_TABLE, theme::node_table()),
                ObjectKind::View => (theme::ICON_VIEW, theme::node_view()),
            };
            spans.push(Span::styled(format!("{icon} "), style));
            spans.push(Span::styled(object.name.clone(), style));
        }

        NodeKind::ColumnNode { column } => {
            let (icon, style) = if column.primary_key {
                (theme::ICON_KEY, theme::node_key_column())
            } else {
                (theme::ICON_COLUMN, theme::node_column())
            };
            spans.push(Span::styled(format!("{icon} "), style));
            spans.push(Span::styled(column.name.clone(), style));
            spans.push(Span::styled(
                format!("  {}", column.type_name),
                theme::dim(),
            ));
            if column.nullable == Some(false) {
                spans.push(Span::styled("  not null", theme::dim()));
            }
        }

        NodeKind::Note { text, is_error } => {
            let style = if *is_error {
                theme::danger()
            } else {
                theme::dim()
            };
            spans.push(Span::styled(text.clone(), style));
        }
    }

    let line = Line::from(spans);
    if selected {
        highlight(line, app.focus == Pane::Explorer, width)
    } else {
        line
    }
}

/// Marks the selected row the way binvim's picker does: an accent bar down the
/// left, then a surface background under the rest. The bar carries the
/// emphasis, so each span keeps its own foreground and the type colours survive
/// being selected.
fn highlight(line: Line<'static>, focused: bool, width: u16) -> Line<'static> {
    let background = theme::selection(focused);
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(line.spans.len() + 1);

    let mut remaining = width as usize;
    for (index, span) in line.spans.into_iter().enumerate() {
        let content: String = span.content.into_owned();
        let trimmed = if UnicodeWidthStr::width(content.as_str()) > remaining {
            ui::truncate(&content, remaining)
        } else {
            content
        };
        remaining = remaining.saturating_sub(UnicodeWidthStr::width(trimmed.as_str()));
        // The bar keeps its own colours; everything after it takes the row
        // background while keeping its foreground.
        let style = if index == 0 {
            span.style
        } else {
            span.style.patch(background)
        };
        spans.push(Span::styled(trimmed, style));
    }

    if remaining > 0 {
        spans.push(Span::styled(" ".repeat(remaining), background));
    }
    Line::from(spans)
}
