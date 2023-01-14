use std::collections::HashMap;

use schemalite::MigrationMetadata;
use tui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

#[derive(Debug, Clone)]
pub struct SchemaView {}

impl SchemaView {
    pub fn new() -> Self {
        Self {}
    }
}

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
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        let items: Vec<ListItem> = state
            .objects
            .iter()
            .map(|i| ListItem::new(i.clone()))
            .collect();

        tui::widgets::StatefulWidget::render(
            List::new(items).highlight_style(Style::default().fg(Color::Green)),
            chunks[0],
            buf,
            &mut state.state,
        );

        tui::widgets::Widget::render(Paragraph::new(state.get_sql().to_owned()), chunks[1], buf);
    }
}

#[derive(Debug, Clone)]
pub struct SchemaState {
    state: ListState,
    objects: Vec<String>,
    sql: Vec<String>,
}

impl SchemaState {
    pub fn from_schema(schema: MigrationMetadata) -> SchemaState {
        let mut objects: Vec<String> = schema.target.tables.keys().map(|k| k.to_owned()).collect();
        objects.sort();
        let sql: Vec<String> = objects
            .iter()
            .map(|o| schema.target.tables.get(o).unwrap().to_owned())
            .collect();

        let mut state = ListState::default();
        if !objects.is_empty() {
            state.select(Some(0));
        }
        SchemaState {
            state,
            objects,
            sql,
        }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.objects.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.objects.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn get_sql(&self) -> &String {
        self.sql.get(self.state.selected().unwrap()).unwrap()
    }
}
