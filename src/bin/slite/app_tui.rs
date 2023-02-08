use color_eyre::Report;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, Debouncer};
use slite::tui::{AppState, BroadcastWriter, ControlFlow, Message, MigratorFactory};
use std::{
    io::{self, Stdout},
    time::Duration,
};
use tokio::sync::mpsc;
use tui::{backend::CrosstermBackend, Frame, Terminal};

struct App<'a> {
    state: AppState<'a>,
    _debouncer: Debouncer<RecommendedWatcher>,
}

impl<'a> App<'a> {
    fn new(
        migrator_factory: MigratorFactory,
        message_tx: mpsc::Sender<Message>,
    ) -> Result<App<'a>, Report> {
        let message_tx_ = message_tx.clone();
        let mut debouncer = new_debouncer(
            Duration::from_millis(250),
            None,
            move |events: Result<_, _>| {
                if events.is_ok() {
                    message_tx_.blocking_send(Message::FileChanged).unwrap();
                }
            },
        )?;
        debouncer
            .watcher()
            .watch(migrator_factory.schema_dir(), RecursiveMode::Recursive)?;

        Ok(App {
            state: AppState::new(migrator_factory, message_tx)?,
            _debouncer: debouncer,
        })
    }
}

pub async fn run_tui(
    migrator_factory: MigratorFactory,
    message_tx: mpsc::Sender<Message>,
    message_rx: mpsc::Receiver<Message>,
) -> Result<(), Report> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new(migrator_factory, message_tx)?;
    let res = run_app(&mut terminal, app, message_rx).await;

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
    mut message_rx: mpsc::Receiver<Message>,
) -> Result<(), Report> {
    let mut event_reader = EventStream::new().fuse();
    let mut log_rx = BroadcastWriter::default().receiver();

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
                    app.state.add_log(log)?;
                }
                while let Ok(log) = log_rx.try_recv() {
                    app.state.add_log(log)?;
                }
            }
            message = message_rx.recv() => {
                if let Some(message) = message {
                    match message {
                        Message::Log(log) => {
                            app.state.add_log(format!("{log}\n"))?;
                        }
                        Message::FileChanged | Message::MigrationCompleted => {
                            app.state.refresh()?;
                        }
                        Message::ConfigChanged(config) => {
                            app.state.update_config(config)?;
                        }
                        Message::SourceChanged(source) => {
                            app.state.set_schema_dir(source)?;
                        }
                        Message::TargetChanged(path) => {
                            app.state.set_target_path(path)?;
                        }

                        _ => {}
                    }
                }
                while let Ok(message) = message_rx.try_recv() {
                    match message {
                        Message::Log(log) => {
                            app.state.add_log(format!("{log}\n"))?;
                        }
                        Message::FileChanged | Message::MigrationCompleted => {
                            app.state.refresh()?;
                        }
                        Message::ConfigChanged(config) => {
                            app.state.update_config(config)?;
                        }
                        Message::SourceChanged(source) => {
                            app.state.set_schema_dir(source)?;
                        }
                        Message::TargetChanged(path) => {
                            app.state.set_target_path(path)?;
                        }

                        _ => {}
                    }
                }
            }
        }
    }
}

fn ui(f: &mut Frame<CrosstermBackend<Stdout>>, app: &mut App) {
    f.render_stateful_widget(slite::tui::App::default(), f.size(), &mut app.state)
}
