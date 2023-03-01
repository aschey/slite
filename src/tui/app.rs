use super::{MigrationMessage, MigrationState, MigratorFactory, SqlState};
use crate::{
    error::{InitializationError, RefreshError, SqlFormatError},
    Config,
};
use elm_ui::{Command, Message, Model, OptionalCommand};
use std::{io::Stdout, marker::PhantomData, path::PathBuf, sync::Arc};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, BorderType, Borders, StatefulWidget, Tabs, Widget},
    Terminal,
};

#[derive(PartialEq, Eq)]
pub enum ControlFlow {
    Quit,
    Continue,
}

#[derive(Clone, Debug)]
pub enum AppMessage {
    FileChanged,
    ConfigChanged(Config),
}

#[derive(Default, Debug)]
pub struct App<'a> {
    phantom: PhantomData<&'a ()>,
}

impl<'a> StatefulWidget for App<'a> {
    type State = AppState<'a>;

    fn render(
        self,
        area: tui::layout::Rect,
        buf: &mut tui::buffer::Buffer,
        state: &mut Self::State,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(area);

        let block = Block::default().style(Style::default());
        block.render(area, buf);

        let titles = state
            .titles
            .iter()
            .map(|t| Spans::from(vec![Span::styled(*t, Style::default().fg(Color::Green))]))
            .collect();
        let tabs = Tabs::new(titles)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .select(state.index as usize)
            .style(Style::default())
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::Black),
            );
        tabs.render(chunks[0], buf);

        match state.index {
            0 => state.source_schema.view(&mut (chunks[1], buf)).unwrap(),
            1 => state.target_schema.view(&mut (chunks[1], buf)).unwrap(),
            2 => state.diff_schema.view(&mut (chunks[1], buf)).unwrap(),
            3 => state.migration.view(&mut (chunks[1], buf)).unwrap(),
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppState<'a> {
    pub titles: Vec<&'a str>,
    pub index: i32,
    source_schema: SqlState<'a>,
    target_schema: SqlState<'a>,
    diff_schema: SqlState<'a>,
    migration: MigrationState<'a>,
}

impl<'a> AppState<'a> {
    pub fn new(migrator_factory: MigratorFactory) -> Result<AppState<'a>, SqlFormatError> {
        let schema = migrator_factory.metadata();
        Ok(AppState {
            titles: vec!["Source", "Target", "Diff", "Migrate"],
            index: 0,
            source_schema: SqlState::schema(schema.source.clone())?,
            target_schema: SqlState::schema(schema.target.clone())?,
            diff_schema: SqlState::diff(schema.clone())?,
            migration: MigrationState::new(migrator_factory),
        })
    }

    pub fn update_config(&mut self, config: Config) -> Result<(), RefreshError> {
        self.migration.migrator_factory().set_config(config);
        self.refresh()
    }

    pub fn set_schema_dir(&mut self, dir: PathBuf) -> Result<(), RefreshError> {
        self.migration.migrator_factory().set_schema_dir(dir);
        self.refresh()
    }

    pub fn set_target_path(&mut self, path: PathBuf) -> Result<(), RefreshError> {
        self.migration.migrator_factory().set_target_path(path);
        self.refresh()
    }

    pub fn refresh(&mut self) -> Result<(), RefreshError> {
        let migrator_factory = self.migration.migrator_factory();
        migrator_factory
            .update_schemas()
            .map_err(RefreshError::InitializationFailure)?;
        let schema = migrator_factory.metadata();

        self.source_schema
            .refresh_schema(schema.source.clone())
            .map_err(RefreshError::SqlFormatFailure)?;

        self.target_schema
            .refresh_schema(schema.target.clone())
            .map_err(RefreshError::SqlFormatFailure)?;

        self.diff_schema
            .refresh_diff(schema.clone())
            .map_err(RefreshError::SqlFormatFailure)?;

        Ok(())
    }

    pub fn next_tab(&mut self) {
        self.index = (self.index + 1).rem_euclid(self.titles.len() as i32);
    }

    pub fn previous_tab(&mut self) {
        self.index = (self.index - 1).rem_euclid(self.titles.len() as i32);
    }

    #[cfg(feature = "crossterm-events")]
    pub fn handle_event(
        &mut self,
        event: &crossterm::event::Event,
    ) -> Result<ControlFlow, InitializationError> {
        use crossterm::event::{Event, KeyCode, KeyEventKind};

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                match (key.code, self.index) {
                    (KeyCode::Char('q'), _) => return Ok(ControlFlow::Quit),
                    (KeyCode::Tab, _) => self.next_tab(),
                    (KeyCode::BackTab, _) => self.previous_tab(),
                    _ => {}
                }
            }
        }

        Ok(ControlFlow::Continue)
    }
}

impl<'a> Model for AppState<'a> {
    type Writer = Terminal<CrosstermBackend<Stdout>>;

    type Error = RefreshError;

    fn init(&mut self) -> Result<OptionalCommand, Self::Error> {
        Ok(self.migration.init().unwrap())
    }

    fn update(&mut self, msg: Arc<elm_ui::Message>) -> Result<OptionalCommand, Self::Error> {
        let mut cmds = vec![];

        match self.index {
            0 => {
                if let Some(cmd) = self.source_schema.update(msg.clone()).unwrap() {
                    cmds.push(cmd);
                }
            }
            1 => {
                if let Some(cmd) = self.target_schema.update(msg.clone()).unwrap() {
                    cmds.push(cmd);
                }
            }
            2 => {
                if let Some(cmd) = self.diff_schema.update(msg.clone()).unwrap() {
                    cmds.push(cmd);
                }
            }
            3 => {
                if let Some(cmd) = self.migration.update(msg.clone()).unwrap() {
                    cmds.push(cmd);
                }
            }
            _ => {}
        }

        match msg.as_ref() {
            Message::TermEvent(e) => {
                let control_flow = self
                    .handle_event(e)
                    .map_err(RefreshError::InitializationFailure)?;
                if control_flow == ControlFlow::Quit {
                    return Ok(Some(Command::quit()));
                }
            }
            Message::Custom(msg) => {
                if let Some(msg) = msg.downcast_ref::<AppMessage>() {
                    match msg {
                        AppMessage::FileChanged => {
                            self.refresh()?;
                        }
                        AppMessage::ConfigChanged(config) => {
                            self.update_config(config.clone())?;
                        }
                    }
                }
                if let Some(MigrationMessage::MigrationCompleted) =
                    msg.downcast_ref::<MigrationMessage>()
                {
                    self.refresh()?;
                }
            }
            _ => {}
        };
        Ok(Some(Command::simple(Message::Batch(cmds))))
    }

    fn view(&self, writer: &mut Self::Writer) -> Result<(), Self::Error> {
        writer
            .draw(|f| f.render_stateful_widget(App::default(), f.size(), &mut self.clone()))
            .map_err(RefreshError::IoFailure)?;
        Ok(())
    }
}
