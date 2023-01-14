use std::collections::HashMap;

use schemalite::{Metadata, SqlPrinter};
use tui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
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
            .constraints([
                Constraint::Length(state.object_view_width as u16),
                Constraint::Min(0),
            ])
            .split(area);
        let items: Vec<ListItem> = state
            .objects
            .iter()
            .map(|i| ListItem::new(" ".to_owned() + i))
            .collect();

        tui::widgets::StatefulWidget::render(
            List::new(items)
                .highlight_style(Style::default().fg(Color::Green))
                .block(Block::default().title("Objects").borders(Borders::ALL)),
            chunks[0],
            buf,
            &mut state.state,
        );
        let mut printer = SqlPrinter::default();
        let formatted_sql = printer.print_spans(state.get_sql());
        tui::widgets::Widget::render(
            Paragraph::new(formatted_sql).block(Block::default().borders(Borders::ALL)),
            chunks[1],
            buf,
        );
    }
}

#[derive(Debug, Clone)]
pub struct SchemaState {
    state: ListState,
    object_view_width: usize,
    objects: Vec<String>,
    sql: Vec<String>,
}

impl SchemaState {
    pub fn from_schema(schema: Metadata) -> SchemaState {
        let mut objects: Vec<String> = schema.tables.keys().map(|k| k.to_owned()).collect();
        objects.sort();
        let max_length = objects
            .iter()
            .map(|o| o.len() + 1)
            .max()
            .unwrap_or_default()
            .max(10);
        let sql: Vec<String> = objects
            .iter()
            .map(|o| schema.tables.get(o).unwrap().to_owned())
            .collect();

        let mut state = ListState::default();
        if !objects.is_empty() {
            state.select(Some(0));
        }
        SchemaState {
            state,
            objects,
            sql,
            object_view_width: max_length,
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
