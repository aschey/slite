use crate::{error::SqlFormatError, Metadata, SqlPrinter};
use ansi_to_tui::IntoText;
use tui::{
    layout::{Constraint, Direction, Layout},
    text::Text,
    widgets::{Block, Borders, Paragraph, StatefulWidget, Wrap},
};

use super::{Objects, ObjectsState};

#[derive(Debug, Clone, Default)]
pub struct SchemaView {}

impl SchemaView {}

impl StatefulWidget for SchemaView {
    type State = SchemaState;

    fn render(
        self,
        area: tui::layout::Rect,
        buf: &mut tui::buffer::Buffer,
        state: &mut Self::State,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(state.state.view_width() as u16),
                Constraint::Min(0),
            ])
            .split(area);

        tui::widgets::StatefulWidget::render(Objects::default(), chunks[0], buf, &mut state.state);

        tui::widgets::Widget::render(
            Paragraph::new(
                state
                    .schema
                    .get(state.state.selected())
                    .expect("Index out of range")
                    .clone(),
            )
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL)),
            chunks[1],
            buf,
        );
    }
}

#[derive(Debug, Clone)]
pub struct SchemaState {
    state: ObjectsState,
    schema: Vec<Text<'static>>,
}

impl SchemaState {
    pub fn new(schema: Metadata) -> Result<Self, SqlFormatError> {
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
        Ok(Self {
            schema: list_items?,
            state,
        })
    }

    pub fn next(&mut self) {
        self.state.next();
    }

    pub fn previous(&mut self) {
        self.state.previous();
    }
}
