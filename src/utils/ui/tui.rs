use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, widgets::Widget};
use std::io::Stdout;

/*-- public --*/

pub type Term = Terminal<CrosstermBackend<Stdout>>;

/// Enter raw mode and return a ready-to-use Terminal.
pub fn setup_terminal() -> anyhow::Result<Term> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(std::io::stdout()))?)
}

/// Restore the terminal to its original state.
pub fn restore_terminal(mut terminal: Term) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Render a single widget to the terminal, then immediately restore.
/// Used by `TerminalOutput` for one-shot non-interactive command output.
///
/// If setup fails (stdout is not a tty, or terminal is too small) the error
/// is returned to the caller, which falls back to `PlainOutput`.
pub fn render_once(widget: impl Widget) -> anyhow::Result<()> {
    let mut terminal = setup_terminal()?;
    terminal.draw(|frame| {
        frame.render_widget(widget, frame.area());
    })?;
    restore_terminal(terminal)?;
    Ok(())
}
