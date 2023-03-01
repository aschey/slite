use std::collections::BTreeMap;

use tui::{
    style::{Color, Modifier, Style},
    text::Text,
    widgets::{Block, List, ListItem, ListState, StatefulWidget},
};

use crate::ObjectType;

#[derive(Debug, Clone)]
pub struct Objects<'a> {
    block: Block<'a>,
}

impl<'a> Objects<'a> {
    pub fn new(block: Block<'a>) -> Self {
        Self { block }
    }
}

impl<'a> StatefulWidget for Objects<'a> {
    type State = ObjectsState;

    fn render(
        self,
        area: tui::layout::Rect,
        buf: &mut tui::buffer::Buffer,
        state: &mut Self::State,
    ) {
        let items: Vec<ListItem> = state.objects.iter().map(|i| i.clone().into()).collect();

        List::new(items)
            .highlight_style(Style::default().fg(Color::Green).bg(Color::Black))
            .block(self.block)
            .render(area, buf, &mut state.state);
    }
}

#[derive(Debug, Clone)]
pub enum ListItemType {
    Entry(String, Color),
    Header(String),
}

impl From<ListItemType> for ListItem<'static> {
    fn from(val: ListItemType) -> Self {
        match val {
            ListItemType::Entry(title, foreground) => ListItem::new(Text::styled(
                "  ".to_owned() + &title,
                Style::default().fg(foreground),
            )),
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
pub struct ObjectsState {
    state: ListState,
    object_view_width: usize,
    objects: Vec<ListItemType>,
    has_items: bool,
    adjusted_index: i32,
    adjusted_size: i32,
}

const LIST_PADDING: usize = 5;
const NUM_HEADERS: i32 = 4;

pub struct StyledObject {
    pub object: String,
    pub foreground: Color,
}

pub struct StyledObjects(BTreeMap<ObjectType, Vec<StyledObject>>);

impl FromIterator<(ObjectType, Vec<StyledObject>)> for StyledObjects {
    fn from_iter<T: IntoIterator<Item = (ObjectType, Vec<StyledObject>)>>(iter: T) -> Self {
        Self(BTreeMap::<ObjectType, Vec<StyledObject>>::from_iter(iter))
    }
}

impl StyledObjects {
    pub fn is_empty(&self) -> bool {
        self.0.values().all(|v| v.is_empty())
    }

    pub fn tables(&self) -> &Vec<StyledObject> {
        self.0.get(&ObjectType::Table).unwrap()
    }

    pub fn indexes(&self) -> &Vec<StyledObject> {
        self.0.get(&ObjectType::Index).unwrap()
    }

    pub fn views(&self) -> &Vec<StyledObject> {
        self.0.get(&ObjectType::View).unwrap()
    }

    pub fn triggers(&self) -> &Vec<StyledObject> {
        self.0.get(&ObjectType::Trigger).unwrap()
    }
}

impl From<&StyledObject> for ListItemType {
    fn from(val: &StyledObject) -> Self {
        ListItemType::Entry(val.object.clone(), val.foreground)
    }
}

impl ObjectsState {
    pub fn new(objects: StyledObjects) -> ObjectsState {
        let has_items = !objects.is_empty();
        let list_items: Vec<_> = vec![]
            .into_iter()
            .chain([ListItemType::Header("Tables".to_owned())])
            .chain(objects.tables().iter().map(Into::into))
            .chain([ListItemType::Header("Indexes".to_owned())])
            .chain(objects.indexes().iter().map(Into::into))
            .chain([ListItemType::Header("Views".to_owned())])
            .chain(objects.views().iter().map(Into::into))
            .chain([ListItemType::Header("Triggers".to_owned())])
            .chain(objects.triggers().iter().map(Into::into))
            .collect();

        let max_length = list_items
            .iter()
            .map(|o| match o {
                ListItemType::Header(header) => header.len(),
                ListItemType::Entry(title, _) => title.len()
            }+LIST_PADDING)
            .max()
            .unwrap_or_default();

        let mut state = ListState::default();
        if has_items {
            state.select(Some(1));
        }
        ObjectsState {
            state,
            adjusted_size: list_items.len() as i32 - NUM_HEADERS,
            objects: list_items,
            object_view_width: max_length,
            has_items,
            adjusted_index: 0,
        }
    }

    pub fn next(&mut self) {
        if !self.has_items {
            return;
        }
        self.adjusted_index = (self.adjusted_index + 1).rem_euclid(self.adjusted_size);

        let mut next_index = (self.state.selected().expect("Item not selected") as i32 + 1)
            .rem_euclid(self.objects.len() as i32);
        let real_index = loop {
            match self.objects.get(next_index as usize) {
                Some(ListItemType::Entry { .. }) => {
                    break next_index;
                }
                Some(ListItemType::Header(_)) => {
                    next_index = (next_index + 1).rem_euclid(self.objects.len() as i32);
                }
                None => unreachable!(),
            }
        };

        self.state.select(Some(real_index as usize));
    }

    pub fn previous(&mut self) {
        if !self.has_items {
            return;
        }
        self.adjusted_index = (self.adjusted_index - 1).rem_euclid(self.adjusted_size);

        let mut next_index = (self.state.selected().expect("Item not selected") as i32 - 1)
            .rem_euclid(self.objects.len() as i32);
        let real_index = loop {
            match self.objects.get(next_index as usize) {
                Some(ListItemType::Entry { .. }) => {
                    break next_index;
                }
                Some(ListItemType::Header(_)) => {
                    next_index = (next_index - 1).rem_euclid(self.objects.len() as i32);
                }
                None => unreachable!(),
            }
        };

        self.state.select(Some(real_index as usize));
    }

    pub fn selected_index(&self) -> usize {
        self.adjusted_index as usize
    }

    pub fn selected_item(&self) -> Option<String> {
        if let Some(selected) = self.state.selected() {
            match self.objects.get(selected).expect("Item not selected") {
                ListItemType::Entry(entry, _) => Some(entry.to_owned()),
                ListItemType::Header(_) => unreachable!(),
            }
        } else {
            None
        }
    }

    pub fn select(&mut self, entry: &str) {
        let mut skip = 0;
        for (i, object) in self.objects.iter().enumerate() {
            match object {
                ListItemType::Header(_) => skip += 1,
                ListItemType::Entry(val, _) => {
                    if val == entry {
                        self.state.select(Some(i));
                        self.adjusted_index = (i - skip) as i32;
                    }
                }
            }
        }
    }

    pub fn view_width(&self) -> usize {
        self.object_view_width
    }
}
