mod app;
mod ui;

use std::io::{self, stdout};
use std::time::Duration;

use crossterm::{
    event::{self, Event},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    if let Err(err) = stdout().execute(EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(err);
    }

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(err) => {
            let _ = disable_raw_mode();
            return Err(err);
        }
    };

    let mut app = App::new();
    let result = run(&mut terminal, &mut app);

    let _ = disable_raw_mode();
    let _ = stdout().execute(LeaveAlternateScreen);
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        if app.should_quit {
            break;
        }

        app.tick_blink();
        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
        {
            app.handle_key(key);
        }
    }
    Ok(())
}
