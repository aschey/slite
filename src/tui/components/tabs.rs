use crossterm::event::KeyCode;
use indexmap::IndexMap;
use rooibos::prelude::*;
use rooibos::reactive::{create_memo, Scope, SignalGet, StoredValue};
use rooibos::runtime::{use_event_context, use_focus_context};

use crate::tui::NUM_HEADERS;

#[derive(Debug, Clone, Copy)]
pub struct Title<'a> {
    pub icon: &'a str,
    pub text: &'a str,
    pub position: usize,
}

#[component]
pub fn HeaderTabs<B: Backend>(
    cx: Scope,
    titles: StoredValue<IndexMap<&'static str, Title<'static>>>,
) -> impl View<B> {
    let focus_context = use_focus_context::<String>(cx);
    let focused_id = focus_context.get_focus_selector();
    focus_context.set_focus(titles.with_value(|t| t.keys().next().copied()));

    let current_tab_index = create_memo(cx, move || {
        let id = focused_id.get().unwrap();
        let title = titles.with_value(|t| t.get(id.as_str()).copied()).unwrap();
        title.position as i32
    });

    let update_current_tab = move |delta: i32| {
        let next_tab = (current_tab_index.get() + delta).rem_euclid(NUM_HEADERS);
        let next = titles.with_value(|t| t.keys().nth(next_tab as usize).copied());
        focus_context.set_focus(next);
    };

    let previous_tab = move || update_current_tab(-1);
    let next_tab = move || update_current_tab(1);

    let event_context = use_event_context(cx);
    event_context.create_key_effect(cx, move |event| match event.code {
        KeyCode::Left => {
            previous_tab();
        }
        KeyCode::Right => {
            next_tab();
        }
        _ => {}
    });

    move || {
        view! { cx,
            <Tabs
                select=current_tab_index.get() as usize
                divider=prop!(<Span style=prop!(<Style fg=Color::Gray/>)>"|"</Span>)
                block=prop! {
                    <Block
                        borders=Borders::BOTTOM
                        border_style=prop!(<Style fg=Color::DarkGray/>)
                        border_type=BorderType::Rounded
                    />}
                > {
                    titles.with_value(|t| t.iter()
                        .map(|(id,t)|
                            title(t.icon, t.text, focused_id.get().as_deref() == Some(id)))
                        .collect())
                }
            </Tabs>
        }
    }
}

fn title<'a>(icon: &'a str, text: &'a str, selected: bool) -> Line<'a> {
    if selected {
        prop! {
            <Line>
                <Span style=prop!(<Style fg=Color::Cyan/>)>{icon}</Span>
                <Span style=prop!(<Style fg=Color::White add_modifier=Modifier::BOLD/>)>
                    {text}
                </Span>
            </Line>
        }
    } else {
        prop! {
            <Line>
                <Span style=prop!(<Style fg=Color::DarkGray/>)>
                    {format!("{icon}{text}")}
                </Span>
            </Line>
        }
    }
}
