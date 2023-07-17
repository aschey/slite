use super::components::{HeaderTabs, HeaderTabsProps, SqlObjects, SqlObjectsProps};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::Backend, backend::CrosstermBackend, Terminal};
use rooibos::{
    components::{Case, Switch, SwitchProps},
    reactive::{create_signal, Scope, Signal, SignalGet},
    EventHandler,
};
use std::io::stdout;
use tui_rsx::{prelude::*, view};

pub async fn run_tui(cx: Scope) {
    enable_raw_mode().unwrap();
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend).unwrap();
    let handler = EventHandler::initialize(cx, terminal);

    let mut v = mount!(cx, <App/>);
    handler.render(move |terminal| {
        terminal
            .draw(|f| {
                v.view(f, f.size());
            })
            .unwrap();
    });

    let mut terminal = handler.run().await;
    disable_raw_mode().unwrap();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).unwrap();
    terminal.show_cursor().unwrap();
}

#[component]
fn App<B: Backend + 'static>(cx: Scope) -> impl View<B> {
    let (selected_tab, set_selected_tab) = create_signal(cx, 0i32);

    move || {
        view! { cx,
            <column>
                <HeaderTabs selected=selected_tab set_selected=set_selected_tab length=2/>
                <TabContent selected_tab=selected_tab/>
            </column>
        }
    }
}

#[component]
fn TabContent<B: Backend + 'static>(
    cx: Scope,
    #[prop(into)] selected_tab: Signal<i32>,
) -> impl View<B> {
    let source_selected = Signal::derive(cx, move || selected_tab.get() == 0);
    let target_selected = Signal::derive(cx, move || selected_tab.get() == 1);
    let diff_selected = Signal::derive(cx, move || selected_tab.get() == 2);

    move || {
        view! {cx,
            <Switch>
                <Case when=move || source_selected.get()>
                    {move || view!(cx, <SqlObjects title="Source" focused=source_selected/>).into_boxed_view()}
                </Case>
                <Case when=move || target_selected.get()>
                    {move || view!(cx, <SqlObjects title="Target" focused=target_selected/>).into_boxed_view()}
                </Case>
                <Case when=move || diff_selected.get()>
                    {move || view!(cx, <SqlObjects title="Diff" focused=diff_selected/>).into_boxed_view()}
                </Case>
            </Switch>
        }
    }
}
