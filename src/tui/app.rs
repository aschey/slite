use super::{MigrationState, MigrationView, MigratorFactory, SqlState, SqlView};
use crate::{
    error::{InitializationError, RefreshError, SqlFormatError},
    Config,
};
use std::{marker::PhantomData, path::PathBuf};
use tokio::sync::mpsc;
use tui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, BorderType, Borders, StatefulWidget, Tabs, Widget},
};

pub enum ControlFlow {
    Quit,
    Continue,
}

#[derive(Clone, Debug)]
pub enum Message {
    Log(String),
    ProcessCompleted,
    MigrationCompleted,
    FileChanged,
    ConfigChanged(Config),
    PathChanged(Option<PathBuf>, Option<PathBuf>),
    SourceChanged(PathBuf, PathBuf),
    TargetChanged(PathBuf, PathBuf),
}

#[derive(Default)]
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
            3 => StatefulWidget::render(MigrationView {}, chunks[1], buf, &mut state.migration),
            _ => {}
        }
    }
}

pub struct AppState<'a> {
    pub titles: Vec<&'a str>,
    pub index: i32,
    source_schema: SqlState,
    target_schema: SqlState,
    diff_schema: SqlState,
    migration: MigrationState,
}

impl<'a> AppState<'a> {
    pub fn new(
        migrator_factory: MigratorFactory,
        message_tx: mpsc::Sender<Message>,
    ) -> Result<AppState<'a>, SqlFormatError> {
        let schema = migrator_factory.metadata();
        Ok(AppState {
            titles: vec!["Source", "Target", "Diff", "Migrate"],
            index: 0,
            source_schema: SqlState::schema(schema.source.clone())?,
            target_schema: SqlState::schema(schema.target.clone())?,
            diff_schema: SqlState::diff(schema.clone())?,
            migration: MigrationState::new(migrator_factory, message_tx),
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
        event: crossterm::event::Event,
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
                    (KeyCode::Down, 3) => self.migration.next(),
                    (KeyCode::Up, 0) => self.source_schema.previous(),
                    (KeyCode::Up, 1) => self.target_schema.previous(),
                    (KeyCode::Up, 2) => self.diff_schema.previous(),
                    (KeyCode::Up, 3) => self.migration.previous(),
                    (KeyCode::Left | KeyCode::Right, 0) => self.source_schema.toggle_focus(),
                    (KeyCode::Left | KeyCode::Right, 1) => self.target_schema.toggle_focus(),
                    (KeyCode::Left | KeyCode::Right, 2) => self.diff_schema.toggle_focus(),
                    (KeyCode::Left | KeyCode::Right, 3) if self.migration.popup_active() => {
                        self.migration.toggle_popup_confirm()
                    }
                    (KeyCode::Left | KeyCode::Right, 3) => self.migration.toggle_focus(),
                    (KeyCode::Enter, 3) => self.migration.execute()?,
                    _ => {}
                }
            }
        }

        Ok(ControlFlow::Continue)
    }

    pub fn add_log(&mut self, log: String) -> Result<(), SqlFormatError> {
        self.migration.add_log(log)
    }
}
