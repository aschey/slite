use std::collections::HashMap;

use schemalite::{Metadata, SqlPrinter};
use tui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Text,
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
        let items: Vec<ListItem> = state.objects.iter().map(|i| i.clone().into()).collect();

        tui::widgets::StatefulWidget::render(
            List::new(items)
                .highlight_style(Style::default().fg(Color::Green))
                .block(Block::default().title("Objects").borders(Borders::ALL)),
            chunks[0],
            buf,
            &mut state.state,
        );
        let mut printer = SqlPrinter::default();
        let formatted_sql = printer.print_spans(state.get_sql().unwrap());
        tui::widgets::Widget::render(
            Paragraph::new(formatted_sql).block(Block::default().borders(Borders::ALL)),
            chunks[1],
            buf,
        );
    }
}

#[derive(Debug, Clone)]
pub enum ListItemType {
    Entry { title: String, sql: String },
    Header(String),
}

impl From<ListItemType> for ListItem<'static> {
    fn from(val: ListItemType) -> Self {
        match val {
            ListItemType::Entry { title, .. } => ListItem::new("  ".to_owned() + &title),
            ListItemType::Header(title) => ListItem::new(Text::styled(
                title,
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SchemaState {
    state: ListState,
    object_view_width: usize,
    objects: Vec<ListItemType>,
    has_items: bool,
    // sql: Vec<String>,
}

impl SchemaState {
    pub fn from_schema(schema: Metadata) -> SchemaState {
        let mut list_items = vec![];
        let mut has_items = false;
        let mut tables: Vec<String> = schema.tables.keys().map(|k| k.to_owned()).collect();
        tables.sort();
        has_items |= !tables.is_empty();
        list_items.push(ListItemType::Header("Tables".to_owned()));

        list_items.extend(tables.into_iter().map(|t| ListItemType::Entry {
            sql: schema.tables.get(&t).unwrap().to_owned(),
            title: t,
        }));

        let mut indexes: Vec<String> = schema.indexes.keys().map(|k| k.to_owned()).collect();
        indexes.sort();
        has_items |= !indexes.is_empty();
        list_items.push(ListItemType::Header("Indexes".to_owned()));

        list_items.extend(indexes.into_iter().map(|t| ListItemType::Entry {
            sql: schema.indexes.get(&t).unwrap().to_owned(),
            title: t,
        }));

        let max_length = list_items
            .iter()
            .map(|o| match o {
                ListItemType::Header(header) => header.len(),
                ListItemType::Entry { title, .. } => title.len()
            }+5)
            .max()
            .unwrap_or_default();

        let mut state = ListState::default();
        if has_items {
            state.select(Some(1));
        }
        SchemaState {
            state,
            objects: list_items,
            object_view_width: max_length,
            has_items,
        }
    }

    pub fn next(&mut self) {
        if !self.has_items {
            return;
        }

        let mut next_index = (self.state.selected().unwrap() + 1) % self.objects.len();
        let adjusted_index = loop {
            match self.objects.get(next_index) {
                Some(ListItemType::Entry { .. }) => {
                    break next_index;
                }
                Some(ListItemType::Header(_)) => {
                    next_index = (next_index + 1) % self.objects.len();
                }
                None => unreachable!(),
            }
        };

        self.state.select(Some(adjusted_index));
    }

    pub fn previous(&mut self) {
        if !self.has_items {
            return;
        }

        let mut next_index = (self.state.selected().unwrap() - 1) % self.objects.len();
        let adjusted_index = loop {
            match self.objects.get(next_index) {
                Some(ListItemType::Entry { .. }) => {
                    break next_index;
                }
                Some(ListItemType::Header(_)) => {
                    next_index = (next_index - 1) % self.objects.len();
                }
                None => unreachable!(),
            }
        };

        self.state.select(Some(adjusted_index));
    }

    fn get_sql(&self) -> Option<&String> {
        if !self.has_items {
            return None;
        }
        if let ListItemType::Entry { sql, .. } =
            self.objects.get(self.state.selected().unwrap()).unwrap()
        {
            return Some(sql);
        }
        None
    }
}
