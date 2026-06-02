use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{App, DirectoryPicker},
    config::FORMATS,
    theme::Theme,
    types::{Focus, Progress, Screen},
};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);

    match app.screen {
        Screen::Form => draw_form(frame, app, chunks[1]),
        Screen::OutputPicker => draw_output_picker(frame, app, chunks[1]),
        Screen::Running => draw_running(frame, app, chunks[1]),
        Screen::Done => draw_done(frame, app, chunks[1]),
    }

    draw_help(frame, app, chunks[2]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let title = Line::from(vec![
        Span::styled(
            "YouTube Downloader TUI",
            Style::default()
                .fg(app.theme.title)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(&app.status, Style::default().fg(app.theme.status)),
    ]);

    let header = Paragraph::new(title)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Left);
    frame.render_widget(header, area);
}

fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    let value_width = usize::from(area.width.saturating_sub(18));
    let theme = &app.theme;
    let rows = vec![
        selectable_line(
            app.focus == Focus::Url,
            "URL",
            if app.url.is_empty() {
                "paste video or playlist URL"
            } else {
                &app.url
            },
            FieldKind::Text,
            value_width,
            theme,
        ),
        selectable_line(
            app.focus == Focus::SourceFormat,
            "Source Format",
            app.source_format_label(),
            FieldKind::Choice,
            value_width,
            theme,
        ),
        selectable_line(
            app.focus == Focus::Resolution,
            "Fallback Res",
            app.resolution_label(),
            if app.source_format_idx == 0 {
                FieldKind::Choice
            } else {
                FieldKind::ReadOnly
            },
            value_width,
            theme,
        ),
        selectable_line(
            app.focus == Focus::Format,
            "Container",
            FORMATS[app.format_idx],
            FieldKind::Choice,
            value_width,
            theme,
        ),
        selectable_line(
            app.focus == Focus::Convert,
            "QuickTime mp4",
            if app.convert { "enabled" } else { "disabled" },
            FieldKind::Toggle(app.convert),
            value_width,
            theme,
        ),
        selectable_line(
            app.focus == Focus::Encoder,
            "Encoder",
            if app.convert {
                app.encoder_mode.label()
            } else {
                "not used"
            },
            if app.convert {
                FieldKind::Choice
            } else {
                FieldKind::ReadOnly
            },
            value_width,
            theme,
        ),
        selectable_line(
            app.focus == Focus::DeleteOriginal,
            "Delete original",
            if !app.convert {
                "not used"
            } else if app.delete_original {
                "enabled"
            } else {
                "disabled"
            },
            if app.convert {
                FieldKind::Toggle(app.delete_original)
            } else {
                FieldKind::ReadOnly
            },
            value_width,
            theme,
        ),
        selectable_line(
            app.focus == Focus::Output,
            "Output",
            &app.output_dir_input,
            FieldKind::Text,
            value_width,
            theme,
        ),
        Line::raw(""),
        button_line(app.focus == Focus::Start, "Start Download", theme),
    ];

    let paragraph = Paragraph::new(rows)
        .block(Block::default().title("Download").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_running(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(5),
        ])
        .split(area);

    let progress = app.progress.clone().unwrap_or(Progress {
        stage: "Running".to_string(),
        ratio: None,
        detail: "Waiting for output...".to_string(),
    });
    let ratio = progress.ratio.unwrap_or(0.0).clamp(0.0, 1.0);
    let label = match progress.ratio {
        Some(value) => format!("{} {:>3.0}%", progress.stage, value * 100.0),
        None => progress.stage,
    };
    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(progress.detail)
                .borders(Borders::ALL),
        )
        .gauge_style(
            Style::default()
                .fg(app.theme.gauge_fg)
                .bg(app.theme.gauge_bg),
        )
        .ratio(ratio)
        .label(label);
    frame.render_widget(gauge, chunks[0]);

    draw_queue(frame, app, chunks[1]);
    draw_logs(frame, app, chunks[2]);
}

fn draw_output_picker(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(5)])
        .split(area);

    let Some(picker) = app.directory_picker.as_ref() else {
        let paragraph = Paragraph::new("No folder picker is active.").block(
            Block::default()
                .title("Output Folder")
                .borders(Borders::ALL),
        );
        frame.render_widget(paragraph, area);
        return;
    };

    draw_output_picker_summary(frame, picker, &app.theme, chunks[0]);
    draw_output_picker_entries(frame, picker, &app.theme, chunks[1]);
}

fn draw_output_picker_summary(
    frame: &mut Frame,
    picker: &DirectoryPicker,
    theme: &Theme,
    area: Rect,
) {
    let mut rows = vec![
        Line::from(vec![
            Span::styled("Current  ", Style::default().fg(theme.label)),
            Span::styled(
                picker.current_dir.display().to_string(),
                Style::default().fg(theme.value),
            ),
        ]),
        Line::from(vec![
            Span::styled("Folders  ", Style::default().fg(theme.label)),
            Span::raw(picker.entries.len().to_string()),
        ]),
    ];

    if let Some(error) = picker.error.as_ref() {
        rows.push(Line::styled(
            error.clone(),
            Style::default().fg(theme.error),
        ));
    }

    let paragraph = Paragraph::new(rows)
        .block(
            Block::default()
                .title("Output Folder")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_output_picker_entries(
    frame: &mut Frame,
    picker: &DirectoryPicker,
    theme: &Theme,
    area: Rect,
) {
    let visible_rows = area.height.saturating_sub(2) as usize;
    let start = scroll_start(picker.selected_idx, visible_rows, picker.entries.len());
    let items = if picker.entries.is_empty() {
        vec![ListItem::new(Line::styled(
            "No folders found",
            Style::default().fg(theme.help),
        ))]
    } else {
        picker
            .entries
            .iter()
            .skip(start)
            .take(visible_rows)
            .enumerate()
            .map(|(visible_index, entry)| {
                let index = start + visible_index;
                let selected = index == picker.selected_idx;
                let marker = if selected { "> " } else { "  " };
                let style = if selected {
                    Style::default()
                        .fg(theme.selected)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.value)
                };

                ListItem::new(Line::from(vec![
                    Span::raw(marker),
                    Span::styled(format!("{}/", entry.name), style),
                ]))
            })
            .collect()
    };

    let list = List::new(items).block(Block::default().title("Folders").borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn scroll_start(selected_idx: usize, visible_rows: usize, total_rows: usize) -> usize {
    if visible_rows == 0 || total_rows <= visible_rows {
        return 0;
    }

    let max_start = total_rows.saturating_sub(visible_rows);
    selected_idx
        .saturating_add(1)
        .saturating_sub(visible_rows)
        .min(max_start)
}

fn draw_queue(frame: &mut Frame, app: &App, area: Rect) {
    let (label, ratio) = if let Some(progress) = app.playlist_progress.as_ref() {
        let ratio = progress.current as f64 / progress.total.max(1) as f64;
        (
            format!("Playlist item {} of {}", progress.current, progress.total),
            ratio.clamp(0.0, 1.0),
        )
    } else {
        ("Single video or playlist queue pending".to_string(), 0.0)
    };

    let gauge = Gauge::default()
        .block(Block::default().title("Queue").borders(Borders::ALL))
        .gauge_style(
            Style::default()
                .fg(app.theme.queue_fg)
                .bg(app.theme.gauge_bg),
        )
        .ratio(ratio)
        .label(label);
    frame.render_widget(gauge, area);
}

fn draw_done(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(6)])
        .split(area);

    let success = app.result_success.unwrap_or(false);
    let color = if success { Color::Green } else { Color::Red };
    let summary = vec![
        Line::from(Span::styled(
            if success { "Completed" } else { "Failed" },
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::raw(app.status.clone()),
        Line::raw(format!("Saved to: {}", app.output_dir.display())),
    ];
    let paragraph = Paragraph::new(summary)
        .block(Block::default().title("Result").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, chunks[0]);

    draw_logs(frame, app, chunks[1]);
}

fn draw_logs(frame: &mut Frame, app: &App, area: Rect) {
    let height = area.height.saturating_sub(2) as usize;
    let start = app.logs.len().saturating_sub(height);
    let items: Vec<ListItem> = app
        .logs
        .iter()
        .skip(start)
        .map(|line| ListItem::new(Line::raw(line.clone())))
        .collect();

    let list = List::new(items).block(Block::default().title("Logs").borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let text = match app.screen {
        Screen::Form => {
            "Tab focus  Left/Right choices  Space toggle  Enter Output browse/start  Esc quit"
        }
        Screen::OutputPicker => {
            "Up/Down choose  Enter open  Backspace parent  ~ home  Space/s select  Esc cancel"
        }
        Screen::Running => {
            "Download is running. Logs update live; q/Esc/Ctrl-C cancels the active process."
        }
        Screen::Done => "Enter/n new download  o open folder  q/Esc quit",
    };
    let help = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(app.theme.help));
    frame.render_widget(help, area);
}

fn selectable_line(
    selected: bool,
    label: &str,
    value: &str,
    kind: FieldKind,
    max_value_width: usize,
    theme: &Theme,
) -> Line<'static> {
    let marker = if selected { "> " } else { "  " };
    let value_style = if selected {
        Style::default()
            .fg(theme.selected)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.value)
    };
    let value = format_field_value(value, kind, selected, max_value_width);

    Line::from(vec![
        Span::raw(marker),
        Span::styled(format!("{label:<14}"), Style::default().fg(theme.label)),
        Span::styled(value, value_style),
    ])
}

#[derive(Clone, Copy)]
enum FieldKind {
    Text,
    Choice,
    Toggle(bool),
    ReadOnly,
}

fn button_line(selected: bool, label: &str, theme: &Theme) -> Line<'static> {
    let style = if selected {
        Style::default()
            .fg(theme.button_fg)
            .bg(theme.button_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.button_unsel)
    };

    Line::from(vec![
        Span::raw(if selected { "> " } else { "  " }),
        Span::styled(format!("[ {label} ]"), style),
    ])
}

fn truncate_middle(value: &str, max_width: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_width {
        return value.to_string();
    }

    if max_width == 0 {
        return String::new();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }

    let keep = max_width - 3;
    let left_count = keep / 2;
    let right_count = keep - left_count;
    let left: String = value.chars().take(left_count).collect();
    let right: String = value
        .chars()
        .rev()
        .take(right_count)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    format!("{left}...{right}")
}

fn format_field_value(value: &str, kind: FieldKind, selected: bool, max_width: usize) -> String {
    match kind {
        FieldKind::Text => {
            let suffix = if selected { "_]" } else { "]" };
            wrap_value(value, "[", suffix, max_width)
        }
        FieldKind::Choice => wrap_value(value, "< ", " >", max_width),
        FieldKind::Toggle(checked) => {
            let prefix = if checked { "[x] " } else { "[ ] " };
            wrap_value(value, prefix, "", max_width)
        }
        FieldKind::ReadOnly => wrap_value(value, "( ", " )", max_width),
    }
}

fn wrap_value(value: &str, prefix: &str, suffix: &str, max_width: usize) -> String {
    let decoration_width = prefix.chars().count() + suffix.chars().count();
    if max_width <= decoration_width {
        return truncate_middle(&format!("{prefix}{value}{suffix}"), max_width);
    }

    let value = truncate_middle(value, max_width - decoration_width);
    format!("{prefix}{value}{suffix}")
}
