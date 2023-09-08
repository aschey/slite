use std::io::stdout;

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use indexmap::IndexMap;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use rooibos::prelude::*;
use rooibos::reactive::{store_value, Scope, SignalGet, StoredValue};
use rooibos::runtime::{provide_focus_context, use_focus_context, EventHandler};

use super::components::{header_tabs, sql_objects, HeaderTabsProps, SqlObjectsProps};
use crate::tui::components::Title;

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
fn App<B: Backend>(cx: Scope) -> impl View<B> {
    provide_focus_context::<String>(cx, Some("source".to_owned()));
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
            <Column>
                <HeaderTabs titles=titles length=2/>
                <TabContent titles=titles/>
            </Column>
        }
    }
}

#[component]
fn TabContent<B: Backend>(
    cx: Scope,
    titles: StoredValue<IndexMap<&'static str, Title<'static>>>,
) -> impl View<B> {
    let focus_context = use_focus_context::<String>(cx);
    let focus_selector = focus_context.get_focus_selector();

    move || {
        view! { cx,
            <Switch>
                {titles.with_value(|t| t.iter().enumerate().map(|(i, (id, title))| {
                    let id = *id;
                    let text = title.text;
                    prop! {
                        <Case key=i when=move || focus_selector.get().as_deref() == Some(id)>
                            {move || view! {cx,
                                <SqlObjects key=i title=text id=id/>
                            }}
                        </Case>
                    }
                }).collect())}
            </Switch>
        }
    }
}
