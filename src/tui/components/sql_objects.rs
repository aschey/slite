use crossterm::event::KeyCode;
use ratatui::{backend::Backend, layout::Rect, style::Color, Frame};
use rooibos::{
    reactive::{create_signal, Scope, Signal, SignalGet, SignalUpdate},
    use_event_context, use_focus_context,
};
use tui_rsx::{prelude::*, view};

use crate::{
    tui::components::{ObjectsList, ObjectsListProps, Sql, SqlProps, StyledObject, StyledObjects},
    ObjectType,
};

#[component]
pub fn SqlObjects<B: Backend + 'static>(
    cx: Scope,
    title: &'static str,
    id: &'static str,
) -> impl View<B> {
    let focus_context = use_focus_context(cx);
    let focused = focus_context.create_focus_handler(id);

    let (focused_index, set_focused_index) = create_signal(cx, 0usize);

    let (objects, set_objects) = create_signal(
        cx,
        StyledObjects::from_iter(vec![
            (
                ObjectType::Table,
                vec![StyledObject {
                    object: "test".to_string(),
                    foreground: Color::Black,
                }],
            ),
            (
                ObjectType::Trigger,
                vec![StyledObject {
                    object: "test".to_string(),
                    foreground: Color::Black,
                }],
            ),
            (
                ObjectType::Index,
                vec![StyledObject {
                    object: "test".to_string(),
                    foreground: Color::Black,
                }],
            ),
            (
                ObjectType::View,
                vec![StyledObject {
                    object: "test".to_string(),
                    foreground: Color::Black,
                }],
            ),
        ]),
    );

    let (sql_view, set_sql_view) = create_signal(cx, "test".to_owned());
    let event_context = use_event_context(cx);

    event_context.create_key_effect(move |key_event| {
        if focused.get() && key_event.code == KeyCode::Tab {
            set_focused_index.update(|s| *s = (*s + 1).rem_euclid(2));
        }
    });

    let objects_focused = Signal::derive(cx, move || focused.get() && focused_index.get() == 0);
    let sql_focused = Signal::derive(cx, move || focused.get() && focused_index.get() == 1);

    move || {
        view! { cx,
            <row>
                <ObjectsList title=title objects=objects focused=objects_focused length=10/>
                <Sql sql_text=sql_view focused=sql_focused/>
            </row>
        }
    }
}
