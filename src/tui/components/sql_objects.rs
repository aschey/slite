use crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;
use rooibos::prelude::*;
use rooibos::reactive::{create_signal, IntoSignal, Scope, SignalGet, SignalUpdate};
use rooibos::runtime::{use_event_context, use_focus_context};

use crate::tui::components::{
    ObjectsList, ObjectsListProps, Sql, SqlProps, StyledObject, StyledObjects,
};
use crate::ObjectType;

#[component]
pub fn SqlObjects<B: Backend>(cx: Scope, title: &'static str, id: &'static str) -> impl View<B> {
    let focus_context = use_focus_context::<String>(cx);
    let focused = focus_context.create_focus_handler(id);

    let focused_index = create_signal(cx, 0usize);

    let (objects, set_objects) = create_signal(
        cx,
        StyledObjects::from_iter(vec![
            (
                ObjectType::Table,
                vec![StyledObject {
                    object: "test".to_string(),
                    foreground: Color::Reset,
                }],
            ),
            (
                ObjectType::Trigger,
                vec![StyledObject {
                    object: "test".to_string(),
                    foreground: Color::Reset,
                }],
            ),
            (
                ObjectType::Index,
                vec![StyledObject {
                    object: "test".to_string(),
                    foreground: Color::Reset,
                }],
            ),
            (
                ObjectType::View,
                vec![StyledObject {
                    object: "test".to_string(),
                    foreground: Color::Reset,
                }],
            ),
        ]),
    )
    .split();

    let (sql_view, set_sql_view) = create_signal(cx, "test".to_owned()).split();
    let event_context = use_event_context(cx);

    event_context.create_key_effect(cx, move |key_event| {
        if focused.get() && key_event.code == KeyCode::Tab {
            focused_index.update(|s| *s = (*s + 1).rem_euclid(2));
        }
    });

    let objects_focused = (move || focused.get() && focused_index.get() == 0).derive_signal(cx);
    let sql_focused = (move || focused.get() && focused_index.get() == 1).derive_signal(cx);

    move || {
        view! { cx,
            <row>
                <ObjectsList title=title objects=objects focused=objects_focused length=10/>
                <Sql sql_text=sql_view focused=sql_focused/>
            </row>
        }
    }
}
