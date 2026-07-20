use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, widgets::Widget};
use std::io::Stdout;

/*-- public --*/

pub type Term = Terminal<CrosstermBackend<Stdout>>;

/// Enter raw mode + alternate screen and return a ready-to-use Terminal.
/// Used by the interactive TUI (`run_interactive_tui`) which owns the full screen.
pub fn setup_terminal() -> anyhow::Result<Term> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(std::io::stdout()))?)
}

/// Restore the terminal after `setup_terminal`.
pub fn restore_terminal(mut terminal: Term) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Render a single widget inline in the main screen buffer, then print a
/// newline so the shell prompt appears below the output.
///
/// This is used by `TerminalOutput` for one-shot command output
/// (`model catalog`, `provider catalog`, etc.). It deliberately does NOT
/// enter the alternate screen — output must persist after the command exits.
///
/// Returns `Err` when stdout is not a tty (pipe / file redirect / CI).
/// The caller (`TerminalOutput`) falls back to `PlainOutput` on error.
pub fn render_once(widget: impl Widget) -> anyhow::Result<()> {
    use crossterm::terminal::size;
    use crossterm::cursor::MoveToColumn;
    use crossterm::terminal::{Clear, ClearType};

    // Fail fast if stdout is not a tty so caller can fall back to plain.
    let _ = size()?;

    // Flush stderr so compiler warnings land before we enter raw mode.
    // Then clear the current line so stray text doesn't bleed into the widget.
    let mut stdout = std::io::stdout();
    execute!(stdout, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;

    enable_raw_mode()?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    // Draw inline (no alternate screen).
    terminal.draw(|frame| {
        frame.render_widget(widget, frame.area());
    })?;

    disable_raw_mode()?;
    // Move cursor below the rendered output so the shell prompt is clean.
    println!();
    Ok(())
}
