use std::collections::BTreeMap;

use crossterm::event::KeyCode;
use rooibos::prelude::*;
use rooibos::reactive::{create_memo, create_signal, ReadSignal, Scope, SignalGet, SignalUpdate};
use rooibos::runtime::use_event_context;

use crate::tui::components::panel;
use crate::tui::NUM_HEADERS;
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
                prop! {
                    <ListItem fg=foreground>
                        {format!(" {title}")}
                    </ListItem>
                }
            }

            ListItemType::Header(title) => {
                prop! {
                    <ListItem>
                        <Text style=prop!(<Style blue bold underlined/>)>
                            {title}
                        </Text>
                    </ListItem>
                }
            }
        }
    }
}

#[component]
pub fn ObjectsList(
    cx: Scope,
    title: &'static str,
    #[prop(into)] focused: ReadSignal<bool>,
    #[prop(into)] objects: ReadSignal<StyledObjects>,
) -> impl View {
    let event_provider = use_event_context(cx);

    let adjusted_index = create_signal(cx, 0i32);
    let real_index = create_signal(cx, 1usize);

    let items = create_memo(cx, move || {
        let objects = objects.get();
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
            .collect::<Vec<_>>()
    });

    let selected_color = move || -> Color {
        match items
            .get()
            .get(real_index.get())
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

        adjusted_index.update(|i| (i + delta).rem_euclid(adjusted_size()));

        let mut next_index = (real_index.get() as i32 + delta).rem_euclid(items.get().len() as i32);
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

    let next = move || adjust_position(1);
    let previous = move || adjust_position(-1);

    event_provider.create_key_effect(cx, move |key_event| {
        if focused.get() {
            match key_event.code {
                KeyCode::Down => {
                    next();
                }
                KeyCode::Up => {
                    previous();
                }
                _ => {}
            }
        }
    });

    move || {
        view! { cx,
            <StatefulList
                block=panel(title, focused.get())
                highlight_style=prop!{
                    <Style
                        fg=selected_color()
                        bg=Color::Black
                        add_modifier=Modifier::BOLD
                    />
                }
                v:state=prop!(<ListState with_selected=Some(real_index.get())/>)
            >
                {items.get().into_iter().map(Into::into).collect::<Vec<_>>()}
            </StatefulList>
        }
    }
}
