use std::collections::BTreeMap;

use rooibos::components::{ListView, WrappingList};
use rooibos::dom::{derive_signal, KeyCode, Render};
use rooibos::reactive::computed::Memo;
use rooibos::reactive::signal::{ReadSignal, RwSignal};
use rooibos::reactive::traits::{Get, Set, Update};
use rooibos::tui::style::{Color, Modifier, Style, Stylize};
use rooibos::tui::widgets::ListItem;

use crate::tui::components::panel;
use crate::ObjectType;

#[derive(Clone)]
pub struct StyledObject {
    pub object: String,
    pub foreground: Color,
}

#[derive(Clone)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListItemType {
    Entry(String, Color),
    Header(String),
}

impl From<ListItemType> for ListItem<'static> {
    fn from(val: ListItemType) -> Self {
        match val {
            ListItemType::Entry(title, foreground) => {
                ListItem::new(format!(" {title}")).fg(foreground)
            }

            ListItemType::Header(title) => {
                ListItem::new(title).style(Style::new().blue().bold().underlined())
            }
        }
    }
}

const NUM_HEADERS: i32 = 4;

pub fn objects_list(title: &'static str, objects: ReadSignal<StyledObjects>) -> impl Render {
    let adjusted_index = RwSignal::new(0i32);
    let real_index = RwSignal::new(Some(1usize));

    let focused = RwSignal::new(false);

    let items = Memo::new(move |_| {
        let objects = objects.get();
        WrappingList(
            vec![]
                .into_iter()
                .chain([ListItemType::Header("Tables".to_owned())])
                .chain(objects.tables().iter().map(Into::into))
                .chain([ListItemType::Header("Indexes".to_owned())])
                .chain(objects.indexes().iter().map(Into::into))
                .chain([ListItemType::Header("Views".to_owned())])
                .chain(objects.views().iter().map(Into::into))
                .chain([ListItemType::Header("Triggers".to_owned())])
                .chain(objects.triggers().iter().map(Into::into))
                .collect::<Vec<_>>(),
        )
    });

    let selected_color = move || -> Color {
        match items
            .get()
            .get(real_index.get().unwrap())
            .expect("Item not selected")
        {
            ListItemType::Entry(_, color) => color.to_owned(),
            ListItemType::Header(_) => unreachable!(),
        }
    };

    let adjusted_size = move || items.get().len() as i32 - NUM_HEADERS;

    let adjust_position = move |delta: i32| {
        if objects.get().is_empty() {
            return;
        }

        adjusted_index.update(|i| *i = (*i + delta).rem_euclid(adjusted_size()));

        let mut next_index =
            (real_index.get().unwrap() as i32 + delta).rem_euclid(items.get().len() as i32);
        let next_real_index = loop {
            match items.get().get(next_index as usize) {
                Some(ListItemType::Entry { .. }) => {
                    break next_index;
                }
                Some(ListItemType::Header(_)) => {
                    next_index = (next_index + delta).rem_euclid(items.get().len() as i32);
                }
                None => unreachable!(),
            }
        };
        real_index.set(next_real_index as usize);
    };

    let block = derive_signal!(panel(title, focused.get()));
    let highlight_style = derive_signal!(
        Style::new()
            .fg(selected_color())
            .bg(Color::Black)
            .add_modifier(Modifier::BOLD)
    );

    let next = move || adjust_position(1);
    let previous = move || adjust_position(-1);

    ListView::new()
        .block(block)
        .highlight_style(highlight_style)
        .on_focus(move |_| {
            focused.set(true);
        })
        .on_blur(move |_| {
            focused.set(false);
        })
        .on_key_down(move |event, _| match event.code {
            KeyCode::Down => {
                next();
            }
            KeyCode::Up => {
                previous();
            }
            _ => {}
        })
        .on_item_click(move |i, v| {
            if matches!(v, ListItemType::Entry(_, _)) {
                real_index.set(Some(i));
            }
        })
        .render(real_index, items)
}
