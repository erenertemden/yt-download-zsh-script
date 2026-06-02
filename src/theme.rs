use ratatui::style::Color;

#[derive(Clone, Copy)]
pub struct Theme {
    pub title: Color,
    pub status: Color,
    pub label: Color,
    pub value: Color,
    pub selected: Color,
    pub gauge_fg: Color,
    pub gauge_bg: Color,
    pub queue_fg: Color,
    pub error: Color,
    pub help: Color,
    pub button_fg: Color,
    pub button_bg: Color,
    pub button_unsel: Color,
}

/// Build the UI theme.
///
/// Body text uses `Color::Reset` so it inherits the terminal's own foreground,
/// which keeps it readable on both light and dark terminal backgrounds. We only
/// pick explicit colors for accents (ANSI names the terminal maps into its own
/// palette), and avoid forcing black/white text — that was tied to the macOS
/// system appearance, which does not match the terminal's actual background.
pub fn detect() -> Theme {
    Theme {
        title: Color::Cyan,
        status: Color::Reset,
        label: Color::Reset,
        value: Color::Reset,
        selected: Color::Magenta,
        gauge_fg: Color::Green,
        gauge_bg: Color::Reset,
        queue_fg: Color::Cyan,
        error: Color::Red,
        help: Color::Reset,
        button_fg: Color::Black,
        button_bg: Color::Green,
        button_unsel: Color::Green,
    }
}
