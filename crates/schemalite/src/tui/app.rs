use std::marker::PhantomData;
use tokio::sync::broadcast;
use tui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, BorderType, Borders, StatefulWidget, Tabs, Widget},
};

use crate::{
    error::{MigrationError, SqlFormatError},
    MigrationMetadata, Migrator, Options,
};

use super::{MigrationMessage, MigrationState, MigrationView, SqlState, SqlView};

pub enum ControlFlow {
    Quit,
    Continue,
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
        schema: MigrationMetadata,
        make_migrator: impl Fn(Options) -> Migrator + 'static,
    ) -> Result<AppState<'a>, SqlFormatError> {
        Ok(AppState {
            titles: vec!["Source", "Target", "Diff", "Migrate"],
            index: 0,
            source_schema: SqlState::schema(schema.source.clone())?,
            target_schema: SqlState::schema(schema.target.clone())?,
            diff_schema: SqlState::diff(schema)?,
            migration: MigrationState::new(make_migrator),
        })
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
    ) -> Result<ControlFlow, MigrationError> {
        use crossterm::event::{Event, KeyCode, KeyEventKind};

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                match (key.code, self.index) {
                    (KeyCode::Char('q'), _) => return Ok(ControlFlow::Quit),
                    (KeyCode::Left | KeyCode::Right, 3) if self.migration.popup_active() => {
                        self.migration.toggle_popup_confirm()
                    }
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
                    (KeyCode::Left | KeyCode::Right, 3) => self.migration.toggle_focus(),
                    (KeyCode::Enter, 3) => self.migration.execute()?,
                    _ => {}
                }
            }
        }

        Ok(ControlFlow::Continue)
    }

    pub fn subscribe_script(&self) -> broadcast::Receiver<MigrationMessage> {
        self.migration.subscribe_script()
    }

    pub fn add_log(&mut self, log: String) -> Result<(), SqlFormatError> {
        self.migration.add_log(log)
    }
}
