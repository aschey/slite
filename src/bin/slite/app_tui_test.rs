use std::path::PathBuf;

use crate::{app::Conf, app_tui::TuiApp};
use confique::Config;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use elm_tui_tester::{terminal_view, TuiTester};
use elm_ui::Message;
use slite::{
    read_extension_dir, read_sql_files,
    tui::{BroadcastWriter, MigratorFactory},
};
use tempfile::TempDir;
use tracing::metadata::LevelFilter;
use tracing_subscriber::{filter::Targets, prelude::*, reload, Layer, Registry};
use tracing_tree2::HierarchicalLayer;
use tui::{backend::TestBackend, style::Color, Terminal};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_load() {
    let backend = TestBackend::new(80, 30);
    let terminal = Terminal::new(backend).unwrap();
    let (app, _tempdir) = setup();

    let tester = TuiTester::new(app, terminal);
    tester
        .wait_for(|term| terminal_view(term).contains("album") && term.get(5, 6).fg == Color::Green)
        .await
        .unwrap();

    tester
        .send_msg(Message::TermEvent(crossterm::event::Event::Key(
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()),
        )))
        .await;
    let (_, view) = tester.wait_for_completion().unwrap();
    let view = terminal_view(&view);
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("../../../test/snapshots");
    settings.bind(|| {
        insta::assert_snapshot!(view);
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scroll_down() {
    let backend = TestBackend::new(80, 30);
    let terminal = Terminal::new(backend).unwrap();
    let (app, _tempdir) = setup();

    let tester = TuiTester::new(app, terminal);
    tester
        .wait_for(|term| terminal_view(term).contains("album") && term.get(5, 6).fg == Color::Green)
        .await
        .unwrap();

    tester
        .send_msg(Message::TermEvent(crossterm::event::Event::Key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        )))
        .await;

    tester
        .wait_for(|term| term.get(6, 6).fg == Color::Green)
        .await
        .unwrap();

    tester
        .send_msg(Message::TermEvent(crossterm::event::Event::Key(
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()),
        )))
        .await;

    let (_, view) = tester.wait_for_completion().unwrap();
    let view = terminal_view(&view);
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("../../../test/snapshots");
    settings.bind(|| {
        insta::assert_snapshot!(view);
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_view_target() {
    let backend = TestBackend::new(80, 30);
    let terminal = Terminal::new(backend).unwrap();
    let (app, _tempdir) = setup();

    let tester = TuiTester::new(app, terminal);
    tester
        .send_msg(Message::TermEvent(crossterm::event::Event::Key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        )))
        .await;

    tester
        .wait_for(|term| term.get(13, 2).bg == Color::Black)
        .await
        .unwrap();

    tester
        .send_msg(Message::TermEvent(crossterm::event::Event::Key(
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()),
        )))
        .await;

    let (_, view) = tester.wait_for_completion().unwrap();
    let view = terminal_view(&view);
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("../../../test/snapshots");
    settings.bind(|| {
        insta::assert_snapshot!(view);
    });
}

fn setup<'a>() -> (TuiApp<'a, TestBackend>, TempDir) {
    let (filter, reload_handle) =
        reload::Layer::new(Targets::default().with_target("slite", LevelFilter::INFO));

    Registry::default()
        .with(
            HierarchicalLayer::default()
                .with_writer(BroadcastWriter::default())
                .with_indent_lines(true)
                .with_level(false)
                .with_filter(filter),
        )
        .try_init()
        .ok();

    let conf = Conf::builder().file("./test/slite.toml").load().unwrap();

    let extensions = conf
        .extension_dir
        .map(read_extension_dir)
        .unwrap()
        .unwrap_or_default();

    let ignore = conf.ignore.map(|i| i.0);
    let before_migration = conf
        .before_migration
        .map(read_sql_files)
        .unwrap_or_default();
    let after_migration = conf.after_migration.map(read_sql_files).unwrap_or_default();
    let config = slite::Config {
        extensions,
        ignore,
        before_migration,
        after_migration,
    };

    let tempdir = tempfile::tempdir().unwrap();
    let app = TuiApp::<TestBackend>::new(
        MigratorFactory::new(
            PathBuf::from("./test"),
            tempdir.path().join("test.db"),
            config,
        )
        .unwrap(),
        reload_handle,
        Conf::default(),
    )
    .unwrap();
    (app, tempdir)
}
