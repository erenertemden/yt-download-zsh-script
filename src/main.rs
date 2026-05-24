mod app;
mod config;
mod job;
mod media;
mod process_control;
mod system;
mod terminal;
mod types;
mod ui;

use terminal::{restore_terminal, run_app, setup_terminal, AppResult};

fn main() -> AppResult<()> {
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}
