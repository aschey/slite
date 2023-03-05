use color_eyre::{eyre, Report};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use elm_ui::{Command, Message, Model, OptionalCommand, Program};
use slite::{
    error::RefreshError,
    tui::{AppState, MigratorFactory, ReloadableConfig},
};
use std::{
    io::{self},
    marker::PhantomData,
    path::PathBuf,
};
use tracing_subscriber::{filter::Targets, reload::Handle, Registry};
use tui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};

use crate::app::{Conf, ConfigStore};

pub enum TuiAppMessage {
    PathChanged(Option<PathBuf>, Option<PathBuf>),
    SourceChanged(PathBuf, PathBuf),
    TargetChanged(PathBuf, PathBuf),
    ConfigCreated(ReloadableConfig<Conf>),
}

#[derive(Debug)]
struct TuiApp<'a, B: Backend> {
    state: AppState<'a>,
    reload_handle: Option<Handle<Targets, Registry>>,
    cli_config: Option<Conf>,
    config: Option<ReloadableConfig<Conf>>,
    _phantom: PhantomData<B>,
}

impl<'a, B: Backend> TuiApp<'a, B> {
    fn new(
        migrator_factory: MigratorFactory,
        reload_handle: Handle<Targets, Registry>,
        cli_config: Conf,
    ) -> Result<TuiApp<'a, B>, Report> {
        Ok(TuiApp {
            state: AppState::new(migrator_factory)?,
            reload_handle: Some(reload_handle),
            cli_config: Some(cli_config),
            config: None,
            _phantom: Default::default(),
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
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = TuiApp::new(migrator_factory, reload_handle, cli_config)?;
    let program = Program::new(app);

    program
        .run(&mut terminal)
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

impl<'a, B: Backend> Model for TuiApp<'a, B> {
    type Writer = Terminal<B>;

    type Error = RefreshError;

    fn init(&mut self) -> Result<OptionalCommand, Self::Error> {
        let cli_config = self.cli_config.take().unwrap();
        let reload_handle = self.reload_handle.take().unwrap();

        let config_cmd = Command::new_blocking(|tx, _| {
            let handler = ConfigStore::new(cli_config, tx, reload_handle);
            let config = ReloadableConfig::new(PathBuf::from("slite.toml"), handler);
            Some(Message::custom(TuiAppMessage::ConfigCreated(config)))
        });

        let mut cmds = vec![config_cmd];
        if let Ok(Some(cmd)) = self.state.init() {
            cmds.push(cmd);
        }
        Ok(Some(Command::simple(Message::Batch(cmds))))
    }

    fn update(&mut self, msg: std::sync::Arc<Message>) -> Result<OptionalCommand, Self::Error> {
        let cmd = self.state.update(msg.clone()).unwrap();
        if let Message::Custom(msg) = msg.as_ref() {
            if let Some(msg) = msg.downcast_ref::<TuiAppMessage>() {
                match (msg, self.config.as_mut()) {
                    (TuiAppMessage::ConfigCreated(config), _) => {
                        self.config = Some(config.clone());
                    }
                    (TuiAppMessage::PathChanged(previous, current), Some(config)) => {
                        config.switch_path(previous.as_deref(), current.as_deref());
                    }
                    (
                        TuiAppMessage::SourceChanged(previous_source, current_source),
                        Some(config),
                    ) => {
                        config.switch_path(Some(previous_source), Some(current_source));
                        self.state.set_schema_dir(current_source.clone())?;
                    }
                    (
                        TuiAppMessage::TargetChanged(previous_target, current_target),
                        Some(config),
                    ) => {
                        config.switch_path(Some(previous_target), Some(current_target));
                        self.state.set_target_path(current_target.clone())?;
                    }
                    _ => {}
                }
            }
        }
        Ok(cmd)
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

#[cfg(test)]
#[path = "./app_tui_test.rs"]
mod app_tui_test;
