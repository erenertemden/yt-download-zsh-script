use std::{io, time::Duration};

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{app::App, ui::draw};

pub type AppResult<T> = Result<T, Box<dyn std::error::Error>>;
type AppTerminal = Terminal<CrosstermBackend<io::Stdout>>;

pub struct TerminalSession {
    terminal: AppTerminal,
    restored: bool,
}

impl TerminalSession {
    pub fn terminal_mut(&mut self) -> &mut AppTerminal {
        &mut self.terminal
    }

    pub fn restore(&mut self) -> AppResult<()> {
        if self.restored {
            return Ok(());
        }

        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        self.restored = true;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if self.restored {
            return;
        }

        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
        self.restored = true;
    }
}

pub fn setup_terminal() -> AppResult<TerminalSession> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(error.into());
    }
    let backend = CrosstermBackend::new(stdout);
    let terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen);
            let _ = disable_raw_mode();
            return Err(error.into());
        }
    };

    Ok(TerminalSession {
        terminal,
        restored: false,
    })
}

pub fn run_app(terminal: &mut AppTerminal) -> AppResult<()> {
    let mut app = App::default();

    loop {
        app.drain_worker_events();
        terminal.draw(|frame| draw(frame, &app))?;

        if event::poll(Duration::from_millis(120))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && app.handle_key(key) {
                    return Ok(());
                }
            }
        }
    }
}
