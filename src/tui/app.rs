use std::io::{self, Stdout};

use indexmap::IndexMap;
use rooibos::components::{Route, Router};
use rooibos::dom::{col, Render};
use rooibos::reactive::owner::StoredValue;
use rooibos::runtime::backend::crossterm::CrosstermBackend;
use rooibos::runtime::{Runtime, RuntimeSettings};

use super::components::header_tabs;
use crate::tui::components::Title;

pub async fn run_tui() -> io::Result<()> {
    let runtime = Runtime::initialize(
        RuntimeSettings::default(),
        CrosstermBackend::<Stdout>::default(),
        app,
    );
    runtime.run().await?;
    Ok(())
}

fn app() -> impl Render {
    let titles = StoredValue::new(IndexMap::from_iter(vec![
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
    ]));

    col![
        Router::new()
            .initial("/source")
            .routes([Route::new("/{tab_id}", move || header_tabs(titles))])
    ]
}
