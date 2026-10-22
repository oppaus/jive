// tui/src/main.rs
use std::io::{self, stdout};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use lib::JEngine;

/// Represents our application state machine
struct App {
    input_buffer: String,
    history: Vec<String>,
    j_engine: JEngine,
}

impl App {
    fn new() -> Self {
        Self {
            input_buffer: String::new(),
            history: vec!["Welcome to the J Language TUI Interface!".to_string(), "Type a J expression and press Enter.".to_string()],
            j_engine: JEngine::new(),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // 1. Initialize terminal raw hooks
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // 2. Instantiate application core state
    let mut app = App::new();
    let mut should_run = true;

    // 3. Execution UI loop
    while should_run {
        // Draw layout
        terminal.draw(|frame| {
            // Split terminal screen into a large History display block and an Input bar
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),    // Output history field
                    Constraint::Length(3), // User command entry box
                ])
                .split(frame.area());

            // Build history string component
            let history_text = app.history.join("\n");
            let history_widget = Paragraph::new(history_text)
                .block(Block::default().title(" J REPL History ").borders(Borders::ALL))
                .wrap(Wrap { trim: false });
            frame.render_widget(history_widget, chunks[0]);

            // Build active input block
            let input_widget = Paragraph::new(app.input_buffer.as_str())
                .block(Block::default().title(" Command Input ").borders(Borders::ALL))
                .style(Style::default().fg(Color::Yellow));
            frame.render_widget(input_widget, chunks[1]);
        })?;

        // 4. Handle standard keyboard event vectors
        if event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                // Prevent duplicate trigger fires on Windows systems
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc => {
                            should_run = false;
                        }
                        KeyCode::Char(c) => {
                            app.input_buffer.push(c);
                        }
                        KeyCode::Backspace => {
                            app.input_buffer.pop();
                        }
                        KeyCode::Enter => {
                            if !app.input_buffer.trim().is_empty() {
                                let command = app.input_buffer.clone();
                                app.history.push(format!("   {}", command));
                                
                                // Direct calculation invocation via our lib FFI engine.
                                // J's own output is captured and returned here.
                                match app.j_engine.eval(&command) {
                                    Ok(output) => {
                                        if output.trim().is_empty() {
                                            // Silent commands (e.g. assignments) print nothing.
                                        } else {
                                            for line in output.lines() {
                                                app.history.push(format!("   {}", line));
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        app.history.push(format!("❌ Error passing data to FFI: {}", err));
                                    }
                                }
                                app.input_buffer.clear();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // 5. Cleanup environment cleanly on exit
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

