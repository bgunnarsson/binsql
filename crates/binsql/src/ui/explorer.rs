use binsql_core::ObjectKind;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::tree::{LoadState, NodeKind};
use crate::app::{App, Pane};
use crate::theme;
use crate::ui;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Pane::Explorer;
    let connected = app.sessions.len();
    let total = app.config.connections.len();
    let block = ui::pane(&format!("Databases {connected}/{total}"), focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.config.connections.is_empty() {
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

    let mut spans = vec![Span::raw("  ".repeat(depth))];

    let expander = match (&node.state, node.expanded) {
        (LoadState::Loading, _) => "◐ ",
        (LoadState::Leaf, _) => "  ",
        (_, true) => "▾ ",
        (_, false) => "▸ ",
    };
    spans.push(Span::styled(expander, theme::dim()));

    match &node.kind {
        NodeKind::Source { name, connected } => {
            let (glyph, glyph_style) = if *connected {
                ("● ", theme::success())
            } else if node.state == LoadState::Loading {
                ("◐ ", theme::warning())
            } else {
                ("○ ", theme::dim())
            };
            spans.push(Span::styled(glyph, glyph_style));
            spans.push(Span::styled(name.clone(), theme::node_source(*connected)));

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
            spans.push(Span::styled(name.clone(), theme::node_catalog(*is_current)));
            if *is_current {
                spans.push(Span::styled("  ·", theme::dim()));
            }
        }

        NodeKind::Schema { name } => {
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
            let style = match object.kind {
                ObjectKind::Table => theme::node_table(),
                ObjectKind::View => theme::node_view(),
            };
            spans.push(Span::styled(object.name.clone(), style));
        }

        NodeKind::ColumnNode { column } => {
            let style = if column.primary_key {
                theme::node_key_column()
            } else {
                theme::node_column()
            };
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
        highlight(line, theme::selection(app.focus == Pane::Explorer), width)
    } else {
        line
    }
}

/// Repaints a line's background for the selection bar, keeping each span's own
/// foreground so the type colours survive being selected.
fn highlight(line: Line<'static>, style: Style, width: u16) -> Line<'static> {
    let used: usize = line
        .spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum();
    let mut spans: Vec<Span<'static>> = line
        .spans
        .into_iter()
        .map(|span| {
            let patched = span
                .style
                .patch(Style::default().bg(style.bg.unwrap_or_default()));
            Span::styled(span.content, patched)
        })
        .collect();

    if used < width as usize {
        spans.push(Span::styled(" ".repeat(width as usize - used), style));
    }
    Line::from(spans)
}
