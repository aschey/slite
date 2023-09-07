use rooibos::prelude::*;
use rooibos::reactive::{ReadSignal, Scope, SignalGet};

use crate::tui::components::panel;

#[component]
pub fn Sql<B: Backend>(
    cx: Scope,
    sql_text: ReadSignal<String>,
    #[prop(into)] focused: ReadSignal<bool>,
) -> impl View<B> {
    move || {
        view! { cx,
            <paragraph block=panel("SQL", focused.get())>
                {sql_text.get()}
            </paragraph>
        }
    }
}
