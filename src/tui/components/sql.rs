use crate::tui::components::panel;
use ratatui::backend::Backend;
use rooibos::reactive::{ReadSignal, Scope, Signal, SignalGet};
use tui_rsx::{prelude::*, view};

#[component]
pub fn Sql<B: Backend + 'static>(
    cx: Scope,
    sql_text: ReadSignal<String>,
    #[prop(into)] focused: Signal<bool>,
) -> impl View<B> {
    move || {
        view! { cx,
            <paragraph block=panel("SQL", focused.get())>
                {sql_text.get()}
            </paragraph>
        }
    }
}
