use color_eyre::Report;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use slite::tui::{
    AppState, BroadcastWriter, ControlFlow, Message, MigratorFactory, ReloadableConfig,
};
use std::io::{self, Stdout};
use tokio::sync::mpsc;
use tui::{backend::CrosstermBackend, Frame, Terminal};

use crate::app::Conf;

struct TuiApp<'a> {
    state: AppState<'a>,
}

impl<'a> TuiApp<'a> {
    fn new(
        migrator_factory: MigratorFactory,
        message_tx: mpsc::Sender<Message>,
    ) -> Result<TuiApp<'a>, Report> {
        Ok(TuiApp {
            state: AppState::new(migrator_factory, message_tx)?,
        })
    }
}

pub async fn run_tui(
    migrator_factory: MigratorFactory,
    message_tx: mpsc::Sender<Message>,
    message_rx: mpsc::Receiver<Message>,
    config: ReloadableConfig<Conf>,
) -> Result<(), Report> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = TuiApp::new(migrator_factory, message_tx)?;
    let res = run_app(&mut terminal, app, message_rx, config).await;

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
    mut app: TuiApp<'static>,
    mut message_rx: mpsc::Receiver<Message>,
    mut config: ReloadableConfig<Conf>,
) -> Result<(), Report> {
    let mut event_reader = EventStream::new().fuse();
    let mut log_rx = BroadcastWriter::default().receiver();

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let mut draw_pending = false;
        loop {
            tokio::select! {
                biased;
                Some(event) = event_reader.next() => {
                    if let ControlFlow::Quit = app.state.handle_event(event?)? {
                        return Ok(())
                    }
                }
                Ok(log) = log_rx.recv() => {
                    app.state.add_log(log)?;
                }
                Some(message) = message_rx.recv() => {
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
                        Message::PathChanged(previous, current) => {
                            config.switch_path(previous.as_deref(), current.as_deref());
                        }
                        Message::SourceChanged(previous_source, current_source) => {
                            config.switch_path(Some(&previous_source), Some(&current_source));
                            app.state.set_schema_dir(current_source)?;
                        }
                        Message::TargetChanged(previous_target, current_target) => {
                            config.switch_path(Some(&previous_target), Some(&current_target));
                            app.state.set_target_path(current_target)?;
                        }
                        _ => {}
                    }
                }
                // If we hit this branch then no other messages are ready, re-draw and wait for the next message
                _ = futures::future::ready(()), if draw_pending => {
                    break;
                }
            }
            draw_pending = true;
        }
    }
}

fn ui(f: &mut Frame<CrosstermBackend<Stdout>>, app: &mut TuiApp) {
    f.render_stateful_widget(slite::tui::App::default(), f.size(), &mut app.state)
}
