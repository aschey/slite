use indexmap::IndexMap;
use rooibos::components::{use_router, KeyedWrappingList, Tab, TabView};
use rooibos::dom::{EventData, KeyCode, KeyEvent, Render};
use rooibos::reactive::owner::StoredValue;
use rooibos::reactive::signal::RwSignal;
use rooibos::reactive::traits::{Get, Set};
use rooibos::tui::layout::Constraint;
use rooibos::tui::style::{Style, Stylize};
use rooibos::tui::text::{Line, Span};
use rooibos::tui::widgets::{Block, Borders};

use super::sql_objects;

#[derive(Debug, Clone, Copy)]
pub struct Title<'a> {
    pub icon: &'a str,
    pub text: &'a str,
    pub position: usize,
}

pub fn header_tabs(titles: StoredValue<IndexMap<&'static str, Title<'static>>>) -> impl Render {
    let router = use_router();
    let current_tab = router.use_param("tab_id");

    let tabs_block = RwSignal::new(
        Block::new()
            .borders(Borders::BOTTOM)
            .border_style(Style::new().dark_gray()),
    );

    let tabs = RwSignal::new(KeyedWrappingList(
        titles
            .with_value(move |t| {
                let t = t.clone();
                t.into_iter().map(move |(id, t)| {
                    Tab::new(Line::from(t.text), id.to_string(), move || {
                        sql_objects(t.text, id)
                    })
                    .decorator(Line::from(t.icon))
                })
            })
            .collect(),
    ));

    let on_key_down = move |key_event: KeyEvent, _: EventData| {
        let tabs = tabs.get();
        match key_event.code {
            KeyCode::Left => {
                if let Some(prev) = tabs.prev_item(&current_tab.get()) {
                    router.push(format!("/{}", prev.get_value()));
                }
            }
            KeyCode::Right => {
                if let Some(next) = tabs.next_item(&current_tab.get()) {
                    router.push(format!("/{}", next.get_value()));
                }
            }
            _ => {}
        }
    };

    TabView::new()
        .header_constraint(Constraint::Length(2))
        .divider(Span::from("|").dark_gray())
        .block(tabs_block)
        .on_key_down(on_key_down)
        .on_title_click(move |_, tab| {
            router.push(format!("/{tab}"));
        })
        .on_focus(move |_| {
            tabs_block.set(
                Block::new()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::new().reset()),
            );
        })
        .on_blur(move |_| {
            tabs_block.set(
                Block::new()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::new().dark_gray()),
            );
        })
        .style(Style::new().white())
        .highlight_style(Style::reset().bold())
        .decorator_highlight_style(Style::reset().cyan())
        .render(current_tab, tabs)
}
