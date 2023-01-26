use crate::{error::SqlFormatError, sql_diff, Metadata, MigrationMetadata, SqlPrinter};
use ansi_to_tui::IntoText;
use tui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Text,
    widgets::{Block, BorderType, Borders, Paragraph, StatefulWidget, Widget, Wrap},
};

use super::{Objects, ObjectsState};

#[derive(Debug, Clone, Default)]
pub struct SqlView {}

impl StatefulWidget for SqlView {
    type State = SqlState;

    fn render(
        self,
        area: tui::layout::Rect,
        buf: &mut tui::buffer::Buffer,
        state: &mut Self::State,
    ) {
        let area_height = area.height - 2;
        if state.height < area_height {
            state.scroll_position = 0;
        }

        if state.height >= area_height && state.scroll_position + area_height >= state.height {
            state.scroll_position = state.height - area_height;
        }

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(state.state.view_width() as u16),
                Constraint::Min(0),
            ])
            .split(area);

        StatefulWidget::render(
            Objects::default().focused(state.focused_index == 0),
            chunks[0],
            buf,
            &mut state.state,
        );

        Widget::render(
            Paragraph::new(
                state
                    .sql
                    .get(state.state.selected())
                    .expect("Selected index out of bounds")
                    .clone(),
            )
            .wrap(Wrap { trim: false })
            .scroll((state.scroll_position, 0))
            .block(
                Block::default()
                    .title("SQL")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(if state.focused_index == 1 {
                        Color::Green
                    } else {
                        Color::White
                    })),
            ),
            chunks[1],
            buf,
        );
    }
}

#[derive(Debug, Clone)]
pub struct SqlState {
    sql: Vec<Text<'static>>,
    state: ObjectsState,
    focused_index: usize,
    height: u16,
    scroll_position: u16,
}

impl SqlState {
    pub fn diff(schemas: MigrationMetadata) -> Result<Self, SqlFormatError> {
        let mut tables: Vec<String> = schemas
            .target
            .tables
            .keys()
            .chain(schemas.source.tables.keys())
            .map(|k| k.to_owned())
            .collect();
        tables.sort();
        tables.dedup();

        let mut indexes: Vec<String> = schemas
            .target
            .indexes
            .keys()
            .chain(schemas.source.indexes.keys())
            .map(|k| k.to_owned())
            .collect();
        indexes.sort();
        indexes.dedup();

        let list_items: Result<Vec<_>, _> = tables
            .iter()
            .map(|t| {
                let diff = sql_diff(
                    &schemas.source.tables.get(t).cloned().unwrap_or_default(),
                    &schemas.target.tables.get(t).cloned().unwrap_or_default(),
                );
                diff.into_text()
                    .map_err(|e| SqlFormatError::TextFormattingFailure(diff, e))
            })
            .chain(indexes.iter().map(|t| {
                let diff = sql_diff(
                    &schemas.source.indexes.get(t).cloned().unwrap_or_default(),
                    &schemas.target.indexes.get(t).cloned().unwrap_or_default(),
                );
                diff.into_text()
                    .map_err(|e| SqlFormatError::TextFormattingFailure(diff, e))
            }))
            .collect();

        let state = ObjectsState::new(tables, indexes);

        Ok(Self::new(list_items?, state))
    }

    pub fn schema(schema: Metadata) -> Result<Self, SqlFormatError> {
        let mut printer = SqlPrinter::default();
        let tables: Vec<String> = schema.tables.keys().map(|k| k.to_owned()).collect();

        let indexes: Vec<String> = schema.indexes.keys().map(|k| k.to_owned()).collect();

        let list_items: Result<Vec<_>, _> = schema
            .tables
            .values()
            .chain(schema.indexes.values())
            .map(|v| {
                printer
                    .print(v)
                    .into_text()
                    .map_err(|e| SqlFormatError::TextFormattingFailure(v.to_owned(), e))
            })
            .collect();

        let state = ObjectsState::new(tables, indexes);
        Ok(Self::new(list_items?, state))
    }

    fn new(sql: Vec<Text<'static>>, state: ObjectsState) -> Self {
        let height = sql.get(0).map(|s| s.height()).unwrap_or(0) as u16;
        Self {
            sql,
            state,
            height,
            focused_index: 0,
            scroll_position: 0,
        }
    }

    pub fn next(&mut self) {
        if self.focused_index == 0 {
            self.state.next();
            self.height = self.sql.get(self.state.selected()).unwrap().height() as u16;
            self.scroll_position = 0;
        } else {
            self.scroll_down();
        }
    }

    pub fn previous(&mut self) {
        if self.focused_index == 0 {
            self.state.previous();
            self.height = self.sql.get(self.state.selected()).unwrap().height() as u16;
            self.scroll_position = 0;
        } else {
            self.scroll_up();
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focused_index = (self.focused_index + 1) % 2;
    }

    fn scroll_up(&mut self) {
        if self.scroll_position > 0 {
            self.scroll_position -= 1;
        }
    }

    fn scroll_down(&mut self) {
        self.scroll_position += 1;
    }
}
