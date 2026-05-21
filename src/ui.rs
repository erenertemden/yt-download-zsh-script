use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::App,
    config::FORMATS,
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
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(&app.status, Style::default().fg(Color::Gray)),
    ]);

    let header = Paragraph::new(title)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Left);
    frame.render_widget(header, area);
}

fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    let rows = vec![
        selectable_line(
            app.focus == Focus::Url,
            "URL",
            if app.url.is_empty() {
                "paste video or playlist URL"
            } else {
                &app.url
            },
        ),
        selectable_line(
            app.focus == Focus::SourceFormat,
            "Source Format",
            app.source_format_label(),
        ),
        selectable_line(
            app.focus == Focus::Resolution,
            "Fallback Res",
            app.resolution_label(),
        ),
        selectable_line(
            app.focus == Focus::Format,
            "Container",
            FORMATS[app.format_idx],
        ),
        selectable_line(
            app.focus == Focus::Convert,
            "QuickTime mp4",
            if app.convert { "enabled" } else { "disabled" },
        ),
        selectable_line(
            app.focus == Focus::Encoder,
            "Encoder",
            if app.convert {
                app.encoder_mode.label()
            } else {
                "not used"
            },
        ),
        selectable_line(
            false,
            "Output",
            app.output_dir
                .to_str()
                .unwrap_or("~/Downloads/youtube_downloads"),
        ),
        Line::raw(""),
        button_line(app.focus == Focus::Start, "Start Download"),
    ];

    let paragraph = Paragraph::new(rows)
        .block(Block::default().title("Download").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_running(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(6)])
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
        .gauge_style(Style::default().fg(Color::Green).bg(Color::Black))
        .ratio(ratio)
        .label(label);
    frame.render_widget(gauge, chunks[0]);

    draw_logs(frame, app, chunks[1]);
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
            "f load formats  Tab/Up/Down focus  Left/Right choose  Space toggle  Enter confirm/start  q quit"
        }
        Screen::Running => {
            "Download is running. Logs update live; q/Esc is disabled until the job finishes."
        }
        Screen::Done => "Enter/n new download  o open folder  q/Esc quit",
    };
    let help = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, area);
}

fn selectable_line<'a>(selected: bool, label: &'a str, value: &'a str) -> Line<'a> {
    let marker = if selected { "> " } else { "  " };
    let value_style = if selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    Line::from(vec![
        Span::raw(marker),
        Span::styled(format!("{label:<14}"), Style::default().fg(Color::Gray)),
        Span::styled(value, value_style),
    ])
}

fn button_line<'a>(selected: bool, label: &'a str) -> Line<'a> {
    let style = if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };

    Line::from(vec![
        Span::raw(if selected { "> " } else { "  " }),
        Span::styled(format!("[ {label} ]"), style),
    ])
}
