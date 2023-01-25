use crate::{Metadata, SqlPrinter};
use ansi_to_tui::IntoText;
use tui::{
    layout::{Constraint, Direction, Layout},
    text::Text,
    widgets::{Block, Borders, Paragraph, StatefulWidget},
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
            Paragraph::new(state.schema.get(state.state.selected()).unwrap().clone())
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
    pub fn new(schema: Metadata) -> Self {
        let mut list_items = vec![];
        let mut printer = SqlPrinter::default();
        let mut tables: Vec<String> = schema.tables.keys().map(|k| k.to_owned()).collect();
        tables.sort();
        let mut indexes: Vec<String> = schema.indexes.keys().map(|k| k.to_owned()).collect();
        indexes.sort();

        list_items.extend(tables.clone().into_iter().map(|t| {
            printer
                .print(schema.tables.get(&t).unwrap())
                .into_text()
                .unwrap()
        }));
        list_items.extend(indexes.clone().into_iter().map(|t| {
            printer
                .print(schema.indexes.get(&t).unwrap())
                .into_text()
                .unwrap()
        }));
        let state = ObjectsState::new(tables, indexes);
        Self {
            schema: list_items,
            state,
        }
    }

    pub fn next(&mut self) {
        self.state.next();
    }

    pub fn previous(&mut self) {
        self.state.previous();
    }
}
