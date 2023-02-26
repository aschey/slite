use super::{MigrationState, MigratorFactory, SqlState, SqlView};
use crate::{
    error::{InitializationError, RefreshError, SqlFormatError},
    Config,
};
use std::{io::Stdout, marker::PhantomData, path::PathBuf, sync::Arc};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, BorderType, Borders, StatefulWidget, Tabs, Widget},
    Terminal,
};
use tui_elm::{Command, Model, OptionalCommand};

#[derive(PartialEq, Eq)]
pub enum ControlFlow {
    Quit,
    Continue,
}

#[derive(Clone, Debug)]
pub enum AppMessage {
    ProcessCompleted,
    MigrationCompleted,
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
        Widget::render(block, area, buf);

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
        Widget::render(tabs, chunks[0], buf);

        match state.index {
            0 => {
                StatefulWidget::render(
                    SqlView::default(),
                    chunks[1],
                    buf,
                    &mut state.source_schema,
                );
            }
            1 => {
                StatefulWidget::render(
                    SqlView::default(),
                    chunks[1],
                    buf,
                    &mut state.target_schema,
                );
            }
            2 => {
                StatefulWidget::render(SqlView::default(), chunks[1], buf, &mut state.diff_schema);
            }
            3 => state.migration.view(&mut (chunks[1], buf)).unwrap(), //StatefulWidget::render(MigrationView {}, chunks[1], buf, &mut state.migration),
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppState<'a> {
    pub titles: Vec<&'a str>,
    pub index: i32,
    source_schema: SqlState,
    target_schema: SqlState,
    diff_schema: SqlState,
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
        self.migration
            .migrator_factory()
            .update_schemas()
            .map_err(RefreshError::InitializationFailure)?;
        let schema = self.migration.migrator_factory().metadata();

        let selected_source = self.source_schema.selected_item();
        self.source_schema =
            SqlState::schema(schema.source.clone()).map_err(RefreshError::SqlFormatFailure)?;
        if let Some(selected_source) = selected_source {
            self.source_schema.select(&selected_source);
        }

        let selected_target = self.target_schema.selected_item();
        self.target_schema =
            SqlState::schema(schema.target.clone()).map_err(RefreshError::SqlFormatFailure)?;
        if let Some(selected_target) = selected_target {
            self.target_schema.select(&selected_target);
        }

        let selected_diff = self.diff_schema.selected_item();
        self.diff_schema =
            SqlState::diff(schema.clone()).map_err(RefreshError::SqlFormatFailure)?;
        if let Some(selected_diff) = selected_diff {
            self.diff_schema.select(&selected_diff);
        }

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
                    (KeyCode::Down, 0) => self.source_schema.next(),
                    (KeyCode::Down, 1) => self.target_schema.next(),
                    (KeyCode::Down, 2) => self.diff_schema.next(),
                    (KeyCode::Up, 0) => self.source_schema.previous(),
                    (KeyCode::Up, 1) => self.target_schema.previous(),
                    (KeyCode::Up, 2) => self.diff_schema.previous(),
                    (KeyCode::Left | KeyCode::Right, 0) => self.source_schema.toggle_focus(),
                    (KeyCode::Left | KeyCode::Right, 1) => self.target_schema.toggle_focus(),
                    (KeyCode::Left | KeyCode::Right, 2) => self.diff_schema.toggle_focus(),
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

    fn update(&mut self, msg: Arc<tui_elm::Message>) -> Result<OptionalCommand, Self::Error> {
        let mut cmds = vec![];
        if self.index == 3 {
            if let Some(cmd) = self.migration.update(msg.clone()).unwrap() {
                cmds.push(cmd);
            }
        }
        match msg.as_ref() {
            tui_elm::Message::TermEvent(e) => {
                let control_flow = self
                    .handle_event(e)
                    .map_err(RefreshError::InitializationFailure)?;
                if control_flow == ControlFlow::Quit {
                    return Ok(Some(Command::quit()));
                }
            }
            tui_elm::Message::Custom(msg) => {
                if let Some(msg) = msg.downcast_ref::<AppMessage>() {
                    match msg {
                        AppMessage::FileChanged | AppMessage::MigrationCompleted => {
                            self.refresh()?;
                        }
                        AppMessage::ConfigChanged(config) => {
                            self.update_config(config.clone())?;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        };
        Ok(Some(tui_elm::Command::simple(tui_elm::Message::Batch(
            cmds,
        ))))
    }

    fn view(&self, writer: &mut Self::Writer) -> Result<(), Self::Error> {
        writer
            .draw(|f| f.render_stateful_widget(App::default(), f.size(), &mut self.clone()))
            .map_err(RefreshError::IoFailure)?;
        Ok(())
    }
}
