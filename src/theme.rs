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

pub fn detect() -> Theme {
    let dark = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "Dark")
        .unwrap_or(false);
    if dark { dark_theme() } else { light_theme() }
}

fn dark_theme() -> Theme {
    Theme {
        title: Color::Cyan,
        status: Color::Gray,
        label: Color::Gray,
        value: Color::White,
        selected: Color::Yellow,
        gauge_fg: Color::Green,
        gauge_bg: Color::Black,
        queue_fg: Color::Cyan,
        error: Color::Red,
        help: Color::DarkGray,
        button_fg: Color::Black,
        button_bg: Color::Green,
        button_unsel: Color::Green,
    }
}

fn light_theme() -> Theme {
    Theme {
        title: Color::Blue,
        status: Color::DarkGray,
        label: Color::DarkGray,
        value: Color::Black,
        selected: Color::Blue,
        gauge_fg: Color::White,
        gauge_bg: Color::Green,
        queue_fg: Color::Blue,
        error: Color::Red,
        help: Color::Gray,
        button_fg: Color::White,
        button_bg: Color::Green,
        button_unsel: Color::Green,
    }
}
