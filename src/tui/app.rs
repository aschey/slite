use crate::tui::components::Title;

use super::components::{HeaderTabs, HeaderTabsProps, SqlObjects, SqlObjectsProps};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use indexmap::IndexMap;
use ratatui::{backend::Backend, backend::CrosstermBackend, Terminal};
use rooibos::{
    prelude::components::{Case, Switch, SwitchProps},
    prelude::*,
    runtime::use_focus_context,
};
use rooibos::{
    reactive::{store_value, Scope, SignalGet, StoredValue},
    runtime::EventHandler,
};
use std::io::stdout;

pub(crate) const NUM_HEADERS: i32 = 4;

pub async fn run_tui(cx: Scope) {
    enable_raw_mode().unwrap();
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend).unwrap();
    let handler = EventHandler::initialize(cx, terminal);

    handler.render(mount!(cx, <App/>));

    let mut terminal = handler.run().await;
    disable_raw_mode().unwrap();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).unwrap();
    terminal.show_cursor().unwrap();
}

#[component]
fn App<B: Backend + 'static>(cx: Scope) -> impl View<B> {
    let titles = store_value(
        cx,
        IndexMap::from_iter(vec![
            (
                "source",
                Title {
                    icon: " ",
                    text: "Source",
                    position: 0,
                },
            ),
            (
                "target",
                Title {
                    icon: " ",
                    text: "Target",
                    position: 1,
                },
            ),
            (
                "diff",
                Title {
                    icon: " ",
                    text: "Diff",
                    position: 2,
                },
            ),
            (
                "migrate",
                Title {
                    icon: " ",
                    text: "Migrate",
                    position: 3,
                },
            ),
        ]),
    );

    move || {
        view! { cx,
            <column>
                <HeaderTabs titles=titles length=2/>
                <TabContent titles=titles/>
            </column>
        }
    }
}

#[component]
fn TabContent<B: Backend + 'static>(
    cx: Scope,
    titles: StoredValue<IndexMap<&'static str, Title<'static>>>,
) -> impl View<B> {
    let focus_context = use_focus_context(cx);
    let focus_selector = focus_context.get_focus_selector();

    move || {
        view! { cx,
            <Switch>
                {titles.with_value(|t| t.iter().enumerate().map(|(i, (id, title))| {
                    let id = *id;
                    let text = title.text;
                    prop! {
                        <Case key=i when=move || focus_selector.get().as_deref() == Some(id)>
                            {move || view!(cx, <SqlObjects key=i title=text id=id/>).into_boxed_view()}
                        </Case>
                    }
                }).collect())}
            </Switch>
        }
    }
}
