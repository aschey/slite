use crate::{sql_diff, MigrationMetadata};
use ansi_to_tui::IntoText;
use tui::{
    layout::{Constraint, Direction, Layout},
    text::Text,
    widgets::{Block, Borders, Paragraph, StatefulWidget},
};

use super::{Objects, ObjectsState};

#[derive(Debug, Clone, Default)]
pub struct DiffView {}

impl StatefulWidget for DiffView {
    type State = DiffState;

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
                    .schema_diffs
                    .get(state.state.selected())
                    .unwrap()
                    .clone(),
            )
            .block(Block::default().borders(Borders::ALL)),
            chunks[1],
            buf,
        );
    }
}

#[derive(Debug, Clone)]
pub struct DiffState {
    schema_diffs: Vec<Text<'static>>,
    state: ObjectsState,
}

impl DiffState {
    pub fn new(schemas: MigrationMetadata) -> Self {
        let mut list_items = vec![];
        let mut tables: Vec<String> = schemas
            .target
            .tables
            .keys()
            .chain(schemas.source.tables.keys())
            .map(|k| k.to_owned())
            .collect();
        tables.sort();
        tables.dedup();

        list_items.extend(tables.clone().into_iter().map(|t| {
            sql_diff(
                &schemas.source.tables.get(&t).cloned().unwrap_or_default(),
                &schemas.target.tables.get(&t).cloned().unwrap_or_default(),
            )
            .into_text()
            .unwrap()
        }));

        let mut indexes: Vec<String> = schemas
            .target
            .indexes
            .keys()
            .chain(schemas.source.indexes.keys())
            .map(|k| k.to_owned())
            .collect();
        indexes.sort();
        indexes.dedup();

        list_items.extend(indexes.clone().into_iter().map(|t| {
            sql_diff(
                &schemas.source.indexes.get(&t).cloned().unwrap_or_default(),
                &schemas.target.indexes.get(&t).cloned().unwrap_or_default(),
            )
            .into_text()
            .unwrap()
        }));

        let state = ObjectsState::new(tables, indexes);

        Self {
            schema_diffs: list_items,
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
