use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::registry::ConfigConstructable;
use crate::utils::ui::backends::plain::PlainOutput;
use crate::utils::ui::output::{HasOutputMetadata, Output, OutputMetadata};
use crate::utils::ui::tui::render_once;

/*-- public --*/

/// The default terminal backend. Renders using ratatui one-shot mode:
/// enable raw mode → draw one frame → restore terminal.
///
/// Falls back to `PlainOutput` automatically when stdout is not a tty
/// (e.g. piped to a file or CI environment).
pub struct TerminalOutput;

impl ConfigConstructable for TerminalOutput {
    fn new(_cfg: &serde_json::Value) -> Self {
        Self
    }
}

impl Output for TerminalOutput {
    fn table(&self, title: &str, headers: &[&str], rows: &[Vec<String>]) {
        // Compute column widths from headers and all row cells
        let col_count = headers.len();
        let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        let header_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);

        let header_cells: Vec<Cell> = headers
            .iter()
            .map(|h| Cell::from(*h).style(header_style))
            .collect();
        let header_row = Row::new(header_cells).height(1);

        let data_rows: Vec<Row> = rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let style = if i % 2 == 0 {
                    Style::default()
                } else {
                    Style::default().bg(Color::Rgb(30, 30, 30))
                };
                let cells: Vec<Cell> = row.iter().map(|c| Cell::from(c.as_str())).collect();
                Row::new(cells).style(style).height(1)
            })
            .collect();

        let constraints: Vec<Constraint> = widths
            .iter()
            .map(|w| Constraint::Length((*w + 2) as u16))
            .collect();

        let table = Table::new(data_rows, constraints)
            .header(header_row)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled(
                        format!(" {} ", title),
                        Style::default().add_modifier(Modifier::BOLD),
                    )),
            )
            .column_spacing(1);

        if render_once(table).is_err() {
            PlainOutput.table(title, headers, rows);
        }
    }

    fn detail(&self, title: &str, fields: &[(&str, String)]) {
        let key_width = fields.iter().map(|(k, _)| k.len()).max().unwrap_or(0);

        let lines: Vec<Line> = fields
            .iter()
            .map(|(k, v)| {
                Line::from(vec![
                    Span::styled(
                        format!("  {:<width$}  ", k, width = key_width),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(v.clone()),
                ])
            })
            .collect();

        let para = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(
                    format!(" {} ", title),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
        );

        if render_once(para).is_err() {
            PlainOutput.detail(title, fields);
        }
    }

    fn status(&self, label: &str, ok: bool, detail: &str) {
        let (mark, colour) = if ok {
            ("✓", Color::Green)
        } else {
            ("✗", Color::Red)
        };
        let line = if detail.is_empty() {
            format!("  {} {}", mark, label)
        } else {
            format!("  {} {}  {}", mark, label, detail)
        };
        let para = Paragraph::new(Line::from(Span::styled(
            line,
            Style::default().fg(colour),
        )));
        if render_once(para).is_err() {
            PlainOutput.status(label, ok, detail);
        }
    }

    fn info(&self, msg: &str) {
        let para = Paragraph::new(msg);
        if render_once(para).is_err() {
            PlainOutput.info(msg);
        }
    }

    fn warn(&self, msg: &str) {
        let para = Paragraph::new(Span::styled(
            format!("Warning: {}", msg),
            Style::default().fg(Color::Yellow),
        ));
        if render_once(para).is_err() {
            PlainOutput.warn(msg);
        }
    }

    fn error(&self, msg: &str) {
        let para = Paragraph::new(Span::styled(
            format!("Error: {}", msg),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        if render_once(para).is_err() {
            PlainOutput.error(msg);
        }
    }
}

impl HasOutputMetadata for TerminalOutput {
    fn metadata() -> OutputMetadata {
        OutputMetadata {
            name: "terminal".to_string(),
            description: "Rich ratatui terminal output with colour and borders".to_string(),
        }
    }
}
