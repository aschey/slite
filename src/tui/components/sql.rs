use rooibos::dom::{widget_ref, Render};
use rooibos::reactive::signal::{ReadSignal, RwSignal};
use rooibos::reactive::traits::{Get, Set};
use rooibos::tui::widgets::Paragraph;

use crate::tui::components::panel;

pub fn sql(sql_text: ReadSignal<String>) -> impl Render {
    let focused = RwSignal::new(false);
    widget_ref!(Paragraph::new(sql_text.get()).block(panel("SQL", focused.get())))
        .on_focus(move |_| {
            focused.set(true);
        })
        .on_blur(move |_| {
            focused.set(false);
        })
}
