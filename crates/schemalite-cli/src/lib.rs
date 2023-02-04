use color_eyre::Report;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use schemalite::{tui::MigrationMessage, Migrator};
use schemalite::{
    tui::{AppState, BroadcastWriter, ControlFlow},
    MigrationMetadata, Options,
};
use std::io::{self, Stdout};
use tui::{backend::CrosstermBackend, Frame, Terminal};

struct App<'a> {
    state: AppState<'a>,
}

impl<'a> App<'a> {
    fn new(
        schema: MigrationMetadata,
        make_migrator: impl Fn(Options) -> Migrator + 'static,
    ) -> Result<App<'a>, Report> {
        Ok(App {
            state: AppState::new(schema, make_migrator)?,
        })
    }
}

pub async fn run_tui(
    schema: MigrationMetadata,
    make_migrator: impl Fn(Options) -> Migrator + 'static,
) -> Result<(), Report> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new(schema, make_migrator)?;
    let res = run_app(&mut terminal, app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    res?;

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut app: App<'static>,
) -> Result<(), Report> {
    let mut event_reader = EventStream::new().fuse();
    let mut log_rx = BroadcastWriter::default().receiver();
    let mut migration_script_rx = app.state.subscribe_script();
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        tokio::select! {
            event = event_reader.next() => {
                if let Some(event) = event {
                    if let ControlFlow::Quit = app.state.handle_event(event?)? {
                        return Ok(())
                    }
                }
            },
            log = log_rx.recv() => {
                if let Ok(log) = log {
                    app.state.add_log(log).unwrap();
                }
                while let Ok(log) = log_rx.try_recv() {
                    app.state.add_log(log).unwrap();
                }
            }
            script = migration_script_rx.recv() => {
                if let Ok(MigrationMessage::Text(script)) = script {
                    app.state.add_log(format!("{script}\n")).unwrap();
                    while let Ok(MigrationMessage::Text(script)) = migration_script_rx.try_recv() {
                        app.state.add_log(format!("{script}\n")).unwrap();
                    }
                }
            }
        }
    }
}

fn ui(f: &mut Frame<CrosstermBackend<Stdout>>, app: &mut App) {
    f.render_stateful_widget(schemalite::tui::App::default(), f.size(), &mut app.state)
}
