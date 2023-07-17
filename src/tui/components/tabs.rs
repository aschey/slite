use crossterm::event::KeyCode;
use ratatui::backend::Backend;
use rooibos::{
    reactive::{ReadSignal, Scope, SignalGet, SignalUpdate, WriteSignal},
    use_event_provider,
};
use tui_rsx::prelude::*;

#[derive(Debug, Clone)]
struct Title<'a> {
    icon: &'a str,
    text: &'a str,
}

#[component]
pub fn HeaderTabs<B: Backend + 'static>(
    cx: Scope,
    selected: ReadSignal<i32>,
    set_selected: WriteSignal<i32>,
) -> impl View<B> {
    let titles = vec![
        Title {
            icon: " ",
            text: "Source",
        },
        Title {
            icon: " ",
            text: "Target",
        },
        Title {
            icon: " ",
            text: "Diff",
        },
        Title {
            icon: " ",
            text: "Migrate",
        },
    ];
    let titles_len = titles.len() as i32;
    let event_provider = use_event_provider(cx);
    event_provider.create_key_effect(move |event| match event.code {
        KeyCode::Left => {
            set_selected.update(|c| *c = (*c - 1).rem_euclid(titles_len));
        }
        KeyCode::Right => {
            set_selected.update(|c| *c = (*c + 1).rem_euclid(titles_len));
        }
        _ => {}
    });

    move || {
        view! { cx,
            <tabs
                select=selected.get() as usize
                divider=prop!(<span style=prop!(<style fg=Color::Gray/>)>"|"</span>)
                block=prop! {
                    <block
                        borders=Borders::BOTTOM
                        border_style=prop!(<style fg=Color::Black/>)
                        border_type=BorderType::Rounded
                    />}
                >
                {titles
                    .iter()
                    .enumerate()
                    .map(|(i, t)| title(t.icon, t.text, i == selected.get() as usize))
                    .collect()}
            </tabs>
        }
    }
}

fn title<'a>(icon: &'a str, text: &'a str, selected: bool) -> Line<'a> {
    if selected {
        prop! {
            <line>
                <span style=prop!(<style fg=Color::Cyan/>)>{icon}</span>
                <span style=prop!(<style fg=Color::White add_modifier=Modifier::BOLD/>)>{text}</span>
            </line>
        }
    } else {
        prop! {
            <line>
                <span style=prop!(<style fg=Color::Black/>)>
                    {format!("{icon}{text}")}
                </span>
            </line>
        }
    }
}
