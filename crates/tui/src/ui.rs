use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use help::{Block as HelpBlock, Inline, PageKind};

use crate::app::{App, Focus, HelpView, HistoryEntry, SearchState};

const PURPLE: Color = Color::Rgb(187, 154, 247);
const GREEN: Color = Color::Rgb(158, 206, 106);
const BLUE: Color = Color::Rgb(122, 162, 247);
const CYAN: Color = Color::Rgb(125, 207, 255);
const ORANGE: Color = Color::Rgb(255, 158, 100);
const RED: Color = Color::Rgb(247, 118, 142);
const GRAY: Color = Color::Rgb(169, 177, 214);
const FG: Color = Color::Rgb(192, 202, 245);
const DIM: Color = Color::Rgb(86, 95, 137);
const SELECTION: Color = Color::Rgb(41, 46, 66);

pub(crate) fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let main = chunks[0];
    let status = chunks[1];

    let (left, right) = if app.help_view().is_some() {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(if main.width >= 80 {
                [Constraint::Percentage(56), Constraint::Percentage(44)]
            } else {
                [Constraint::Percentage(50), Constraint::Percentage(50)]
            })
            .split(main);
        (split[0], split.get(1).copied())
    } else {
        (main, None)
    };

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(left);
    let history_area = left_chunks[0];
    let input_area = left_chunks[1];

    draw_history(frame, app, history_area);
    draw_input(frame, app, input_area);
    if let Some(help_area) = right {
        draw_help(frame, app, help_area);
    }
    draw_status(frame, app, status);
    if app.search.is_some() {
        draw_search(frame, app, area);
    }
}

fn draw_history(frame: &mut Frame, app: &mut App, area: Rect) {
    let lines = history_lines(app);
    let block = Block::default()
        .title(" J REPL History ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIM))
        .title_style(Style::default().fg(GRAY));
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    let total = paragraph.line_count(area.width);
    let max_scroll = total.saturating_sub(area.height as usize);
    let max_offset = u16::try_from(max_scroll).unwrap_or(u16::MAX);
    app.history_offset_from_bottom = app.history_offset_from_bottom.min(max_offset);
    let top = max_scroll.saturating_sub(app.history_offset_from_bottom as usize);
    let top = u16::try_from(top).unwrap_or(u16::MAX);

    frame.render_widget(paragraph.scroll((top, 0)), area);
}

fn history_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for entry in &app.history {
        match entry {
            HistoryEntry::Input(command) => {
                lines.push(Line::from(vec![
                    Span::styled("=> ", Style::default().fg(GREEN)),
                    Span::styled(command.clone(), Style::default().fg(PURPLE)),
                ]));
            }
            HistoryEntry::Output(output) => {
                lines.push(Line::from(Span::styled(
                    format!("   {output}"),
                    Style::default().fg(FG),
                )));
            }
            HistoryEntry::Info(info) => {
                lines.push(Line::from(Span::styled(info.clone(), Style::default().fg(GRAY))));
            }
            HistoryEntry::Error(error) => {
                lines.push(Line::from(vec![
                    Span::styled("! ", Style::default().fg(RED)),
                    Span::styled(error.clone(), Style::default().fg(RED)),
                ]));
            }
        }
    }
    lines
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Input;
    let title = if app.help_view().is_some() {
        " Command Input (Tab to focus) "
    } else {
        " Command Input "
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { PURPLE } else { DIM }))
        .title_style(Style::default().fg(if focused { PURPLE } else { GRAY }));

    let inner_x = area.x.saturating_add(1);
    let inner_y = area.y.saturating_add(1);
    let inner_width = area.width.saturating_sub(2);
    let cursor = app.cursor.min(app.input_buffer.len());
    let cursor_width = UnicodeWidthStr::width(&app.input_buffer[..cursor]);
    let visible_width = inner_width.saturating_sub(1) as usize;
    let horizontal_offset = cursor_width.saturating_sub(visible_width);
    let input_style = if app.input_buffer.starts_with('/') {
        Style::default().fg(CYAN)
    } else {
        Style::default().fg(PURPLE)
    };

    let paragraph = Paragraph::new(Line::from(Span::styled(
        app.input_buffer.clone(),
        input_style,
    )))
    .block(block)
    .scroll((0, u16::try_from(horizontal_offset).unwrap_or(u16::MAX)));
    frame.render_widget(paragraph, area);

    if focused && app.cursor_visible && app.search.is_none() && inner_width > 0 {
        let x = inner_x
            + u16::try_from(cursor_width.saturating_sub(horizontal_offset))
                .unwrap_or(u16::MAX)
                .min(inner_width.saturating_sub(1));
        frame.set_cursor_position((x, inner_y));
    }
}

fn draw_help(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(view) = app.help_view() else {
        return;
    };

    let title = format!(" Help: {} ", view.title);
    let lines = help_lines(view, app.help_example_index());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(CYAN))
        .title_style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    let total = paragraph.line_count(area.width);
    let max_scroll = total.saturating_sub(area.height as usize);
    let max_scroll = u16::try_from(max_scroll).unwrap_or(u16::MAX);
    app.help_scroll = app.help_scroll.min(max_scroll);

    frame.render_widget(paragraph.scroll((app.help_scroll, 0)), area);
}

fn help_lines(view: &HelpView, example_index: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        view.subtitle.clone(),
        Style::default().fg(DIM),
    )));
    if !view.url.is_empty() {
        lines.push(Line::from(Span::styled(
            view.url.clone(),
            Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
        )));
    }
    lines.push(Line::from(""));

    for block in &view.blocks {
        match block {
            HelpBlock::Heading { level, spans } => {
                push_blank(&mut lines);
                let color = match level {
                    1 | 2 => BLUE,
                    3 => CYAN,
                    _ => PURPLE,
                };
                let style = Style::default()
                    .fg(color)
                    .add_modifier(Modifier::BOLD);
                lines.extend(spans_to_lines(spans, style));
            }
            HelpBlock::Para { spans } => {
                lines.extend(spans_to_lines(spans, Style::default().fg(FG)));
            }
            HelpBlock::Code { text } => {
                push_blank(&mut lines);
                for line in text.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {line}"),
                        Style::default().fg(GREEN),
                    )));
                }
                push_blank(&mut lines);
            }
            HelpBlock::Table { rows } => {
                if rows.is_empty() {
                    continue;
                }
                push_blank(&mut lines);
                for (row_index, row) in rows.iter().enumerate() {
                    let mut spans = Vec::new();
                    for (cell_index, cell) in row.iter().enumerate() {
                        if cell_index > 0 {
                            spans.push(Span::styled(" │ ", Style::default().fg(DIM)));
                        }
                        let style = if row_index == 0 {
                            Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(FG)
                        };
                        spans.push(Span::styled(cell.replace('\n', " / "), style));
                    }
                    lines.push(Line::from(spans));
                }
                push_blank(&mut lines);
            }
            HelpBlock::List { items } => {
                for item in items {
                    let item_lines = spans_to_lines(item, Style::default().fg(FG));
                    for (index, mut line) in item_lines.into_iter().enumerate() {
                        if index == 0 {
                            line.spans
                                .insert(0, Span::styled("• ", Style::default().fg(CYAN)));
                        }
                        lines.push(line);
                    }
                }
            }
        }
    }

    if !view.examples.is_empty() {
        push_blank(&mut lines);
        let index = example_index.min(view.examples.len().saturating_sub(1));
        lines.push(Line::from(vec![
            Span::styled("Example ", Style::default().fg(ORANGE).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{}/{}", index + 1, view.examples.len()),
                Style::default().fg(DIM),
            ),
        ]));
        if let Some(example) = view.examples.get(index) {
            lines.push(Line::from(Span::styled(
                format!("  {example}"),
                Style::default().fg(GREEN),
            )));
        }
    }

    lines
}

fn push_blank(lines: &mut Vec<Line<'static>>) {
    if lines.last().is_some_and(|line| !line.spans.is_empty()) {
        lines.push(Line::from(""));
    }
}

fn spans_to_lines(spans: &[Inline], base: Style) -> Vec<Line<'static>> {
    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    for span in spans {
        let style = inline_style(span, base);
        let mut first = true;
        for part in span.text.split('\n') {
            if !first {
                lines.push(Vec::new());
            }
            if !part.is_empty()
                && let Some(line) = lines.last_mut()
            {
                line.push(Span::styled(part.to_string(), style));
            }
            first = false;
        }
    }
    lines.into_iter().map(Line::from).collect()
}

fn inline_style(span: &Inline, base: Style) -> Style {
    let mut style = base;
    if span.code {
        style = style.fg(GREEN);
    }
    if span.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if span.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    style
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let line = Line::from(Span::styled(
        format!(" {}", app.status_line()),
        Style::default().fg(GRAY),
    ));
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_search(frame: &mut Frame, app: &App, area: Rect) {
    let Some(search) = app.search.as_ref() else {
        return;
    };
    let popup = centered_popup(area, 82, 72);
    frame.render_widget(Clear, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(popup);

    let input_block = Block::default()
        .title(" Search J Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(CYAN))
        .title_style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
    let input = Paragraph::new(Line::from(vec![
        Span::styled(
            "Search: ",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(search.query.clone(), Style::default().fg(PURPLE)),
        Span::styled("▌", Style::default().fg(PURPLE)),
    ]))
    .block(input_block);
    frame.render_widget(input, chunks[0]);

    draw_search_results(frame, search, chunks[1]);
}

fn draw_search_results(frame: &mut Frame, search: &SearchState, area: Rect) {
    let visible = area.height.saturating_sub(2) as usize;
    let mut lines = Vec::new();

    if search.hits.is_empty() {
        let message = if search.query.trim().is_empty() {
            "Type a symbol, name, or phrase."
        } else {
            "No matching pages."
        };
        lines.push(Line::from(Span::styled(
            message,
            Style::default().fg(DIM),
        )));
    } else {
        for (index, hit) in search
            .hits
            .iter()
            .enumerate()
            .skip(search.scroll)
            .take(visible.max(1))
        {
            let selected = index == search.selected;
            let line_style = if selected {
                Style::default().bg(SELECTION)
            } else {
                Style::default()
            };
            let mut line = Line::from(vec![
                Span::styled(
                    format!("[{}] ", hit.kind.badge()),
                    Style::default()
                        .fg(kind_color(hit.kind))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    hit.title.clone(),
                    Style::default().fg(FG).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}", truncate_chars(&hit.snippet, 80)),
                    Style::default().fg(DIM),
                ),
            ]);
            line.style = line_style;
            lines.push(line);
        }
    }

    let title = format!(" {} result{} ", search.hits.len(), if search.hits.len() == 1 { "" } else { "s" });
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIM))
        .title_style(Style::default().fg(GRAY));
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn centered_popup(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let width = area
        .width
        .saturating_mul(width_percent)
        .saturating_div(100)
        .max(40)
        .min(area.width);
    let height = area
        .height
        .saturating_mul(height_percent)
        .saturating_div(100)
        .max(10)
        .min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn kind_color(kind: PageKind) -> Color {
    match kind {
        PageKind::Primitive => CYAN,
        PageKind::Concept => BLUE,
        PageKind::Phrase => PURPLE,
        PageKind::Idiom => ORANGE,
    }
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out = String::new();
    for ch in value.chars().take(max) {
        out.push(ch);
    }
    out.push('…');
    out
}
