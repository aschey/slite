use rooibos::prelude::*;
use rooibos::reactive::{ReadSignal, Scope, SignalGet};

use crate::tui::components::panel;

#[component]
pub fn Sql(
    cx: Scope,
    sql_text: ReadSignal<String>,
    #[prop(into)] focused: ReadSignal<bool>,
) -> impl View {
    move || {
        view! { cx,
            <Paragraph block=panel("SQL", focused.get())>
                {sql_text.get()}
            </Paragraph>
        }
    }
}
