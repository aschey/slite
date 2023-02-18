use color_eyre::{eyre, Report};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use slite::{
    error::RefreshError,
    tui::{AppState, BroadcastWriter, ControlFlow, MigratorFactory, ReloadableConfig},
};
use std::{
    fmt::Debug,
    io::{self, Stdout},
    path::PathBuf,
};
use tokio::sync::mpsc;
use tracing_subscriber::{filter::Targets, reload::Handle, Registry};
use tui::{backend::CrosstermBackend, Frame, Terminal};
use tui_elm::{Model, Program};

use crate::app::{Conf, ConfigStore};

pub enum TuiAppMessage {
    PathChanged(Option<PathBuf>, Option<PathBuf>),
    SourceChanged(PathBuf, PathBuf),
    TargetChanged(PathBuf, PathBuf),
}

struct TuiApp<'a> {
    state: AppState<'a>,
    config: ReloadableConfig<Conf>,
}

impl<'a> TuiApp<'a> {
    fn new(
        migrator_factory: MigratorFactory,
        config: ReloadableConfig<Conf>,
    ) -> Result<TuiApp<'a>, Report> {
        Ok(TuiApp {
            state: AppState::new(migrator_factory)?,
            config,
        })
    }
}

pub async fn run_tui(
    migrator_factory: MigratorFactory,
    cli_config: Conf,
    reload_handle: Handle<Targets, Registry>,
) -> Result<(), Report> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = TuiApp::new(migrator_factory, config)?;
    let program = Program::new(app);

    let handler = ConfigStore::new(program.msg_tx(), cli_config, reload_handle);
    let reloadable = ReloadableConfig::new(PathBuf::from("slite.toml"), handler);
    program
        .run(&mut terminal)
        .await
        .map_err(|e| eyre::eyre!("err"))?;
    //let res = run_app(&mut terminal, app, message_rx, config).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    Ok(())
}

impl<'a> Model for TuiApp<'a> {
    type Writer = Terminal<CrosstermBackend<Stdout>>;

    type Error = RefreshError;

    fn init(&self) -> Result<tui_elm::OptionalCommand, Self::Error> {
        Ok(None)
    }

    fn update(
        &mut self,
        msg: std::sync::Arc<tui_elm::Message>,
    ) -> Result<tui_elm::OptionalCommand, Self::Error> {
        self.state.update(msg.clone()).unwrap();
        match msg.as_ref() {
            tui_elm::Message::Custom(msg) => {
                if let Some(msg) = msg.downcast_ref::<TuiAppMessage>() {
                    match msg {
                        TuiAppMessage::PathChanged(previous, current) => {
                            self.config
                                .switch_path(previous.as_deref(), current.as_deref());
                        }
                        TuiAppMessage::SourceChanged(previous_source, current_source) => {
                            self.config
                                .switch_path(Some(&previous_source), Some(&current_source));
                            self.state.set_schema_dir(*current_source)?;
                        }
                        TuiAppMessage::TargetChanged(previous_target, current_target) => {
                            self.config
                                .switch_path(Some(&previous_target), Some(&current_target));
                            self.state.set_target_path(*current_target)?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn view(&self, writer: &mut Self::Writer) -> Result<(), Self::Error> {
        writer
            .draw(|f| {
                f.render_stateful_widget(
                    slite::tui::App::default(),
                    f.size(),
                    &mut self.state.clone(),
                )
            })
            .unwrap();
        Ok(())
    }
}

// async fn run_app(
//     terminal: &mut Terminal<CrosstermBackend<Stdout>>,
//     mut app: TuiApp<'static>,
//     mut message_rx: mpsc::Receiver<AppMessage>,
//     mut config: ReloadableConfig<Conf>,
// ) -> Result<(), Report> {
//     let mut event_reader = EventStream::new().fuse();
//     let mut log_rx = BroadcastWriter::default().receiver();

//     loop {
//         terminal.draw(|f| ui(f, &mut app))?;

//         let mut draw_pending = false;
//         loop {
//             tokio::select! {
//                 biased;
//                 Some(event) = event_reader.next() => {
//                     if let ControlFlow::Quit = app.state.handle_event(event?)? {
//                         return Ok(())
//                     }
//                 }
//                 Ok(log) = log_rx.recv() => {
//                   //  app.state.add_log(log)?;
//                 }
//                 Some(message) = message_rx.recv() => {
//                     match message {
//                         // AppMessage::Log(log) => {
//                         //     app.state.add_log(format!("{log}\n"))?;
//                         // }
//                         // AppMessage::FileChanged | AppMessage::MigrationCompleted => {
//                         //     app.state.refresh()?;
//                         // }
//                         // AppMessage::ConfigChanged(config) => {
//                         //     app.state.update_config(config)?;
//                         // }
//                         AppMessage::PathChanged(previous, current) => {
//                             config.switch_path(previous.as_deref(), current.as_deref());
//                         }
//                         AppMessage::SourceChanged(previous_source, current_source) => {
//                             config.switch_path(Some(&previous_source), Some(&current_source));
//                             app.state.set_schema_dir(current_source)?;
//                         }
//                         AppMessage::TargetChanged(previous_target, current_target) => {
//                             config.switch_path(Some(&previous_target), Some(&current_target));
//                             app.state.set_target_path(current_target)?;
//                         }
//                         _ => {}
//                     }
//                 }
//                 // If we hit this branch then no other messages are ready, re-draw and wait for the next message
//                 _ = futures::future::ready(()), if draw_pending => {
//                     break;
//                 }
//             }
//             draw_pending = true;
//         }
//     }
// }

// fn ui(f: &mut Frame<CrosstermBackend<Stdout>>, app: &mut TuiApp) {
//     f.render_stateful_widget(slite::tui::App::default(), f.size(), &mut app.state)
// }
