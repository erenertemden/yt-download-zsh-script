mod app;
mod config;
mod job;
mod media;
mod process_control;
mod system;
mod terminal;
mod theme;
mod types;
mod ui;

use terminal::{run_app, setup_terminal, AppResult};

fn main() -> AppResult<()> {
    let mut terminal = setup_terminal()?;
    let result = run_app(terminal.terminal_mut());
    terminal.restore()?;
    result
}
