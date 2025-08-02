use confique::Config;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use elm_ui_tester::{TerminalView, UiTester};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use serial_test::serial;
use slite::tui::{BroadcastWriter, MigratorFactory};
use slite::{read_extension_dir, read_sql_files};
use tempfile::TempDir;
use tracing::metadata::LevelFilter;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{Layer, Registry, reload};
use tracing_tree2::HierarchicalLayer;

use crate::app::Conf;
use crate::app_tui::TuiApp;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_load() {
    let (tester, _tempdir) = setup(80, 50);
    tester
        .wait_for(|term| {
            term.terminal_view().contains("album") && term.cell((5, 5)).unwrap().bg == Color::Black
        })
        .await
        .unwrap();
    tester
        .send_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()))
        .await;
    let (_, view) = tester.wait_for_completion().unwrap();
    let view = view.terminal_view();
    insta::with_settings!({
        snapshot_path => "../../../test/snapshots"
    }, {
        insta::assert_snapshot!(view);
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scroll_down() {
    let (tester, _tempdir) = setup(80, 50);
    tester
        .wait_for(|term| term.cell((5, 5)).unwrap().bg == Color::Black)
        .await
        .unwrap();
    tester
        .send_key(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()))
        .await;
    tester
        .wait_for(|term| term.cell((5, 6)).unwrap().bg == Color::Black)
        .await
        .unwrap();
    tester
        .send_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()))
        .await;
    let (_, view) = tester.wait_for_completion().unwrap();
    let view = view.terminal_view();
    insta::with_settings!({
        snapshot_path => "../../../test/snapshots"
    }, {
        insta::assert_snapshot!(view);
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_view_target() {
    let (tester, _tempdir) = setup(80, 50);
    tester
        .send_key(KeyEvent::new(KeyCode::Right, KeyModifiers::empty()))
        .await;
    tester
        .wait_for(|term| term.cell((15, 1)).unwrap().fg == Color::White)
        .await
        .unwrap();
    tester
        .send_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()))
        .await;
    let (_, view) = tester.wait_for_completion().unwrap();
    let view = view.terminal_view();
    insta::with_settings!({
        snapshot_path => "../../../test/snapshots"
    }, {
        insta::assert_snapshot!(view);
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn test_dry_run() {
    let (tester, _tempdir) = setup(80, 60);
    for _ in 0..3 {
        tester
            .send_key(KeyEvent::new(KeyCode::Right, KeyModifiers::empty()))
            .await;
    }
    tester
        .wait_for(|term| term.terminal_view().contains("Controls"))
        .await
        .unwrap();
    tester
        .send_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
        .await;
    tester
        .wait_for(|term| term.terminal_view().contains("Migration completed"))
        .await
        .map_err(|e| e.terminal_view())
        .unwrap();
    tester
        .send_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()))
        .await;
    let (_, view) = tester.wait_for_completion().unwrap();
    let view = view.terminal_view();

    insta::with_settings!({
        snapshot_path => "../../../test/snapshots",
        filters => vec![(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}", "yyyy-mm-dd hh:mm:dd")]
    }, {
        insta::assert_snapshot!(view);
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn test_generate_script() {
    let (tester, _tempdir) = setup(100, 200);

    for _ in 0..3 {
        tester
            .send_key(KeyEvent::new(KeyCode::Right, KeyModifiers::empty()))
            .await;
    }
    tester
        .wait_for(|term| term.terminal_view().contains("Controls"))
        .await
        .map_err(|e| e.terminal_view())
        .unwrap();

    tester
        .send_key(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()))
        .await;

    tester
        .wait_for(|term| {
            term.cell((5, 6))
                .unwrap()
                .modifier
                .contains(Modifier::REVERSED)
        })
        .await
        .map_err(|e| e.terminal_view())
        .unwrap();
    tester
        .send_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
        .await;

    tester
        .wait_for(|term| {
            term.cell((5, 6))
                .unwrap()
                .modifier
                .contains(Modifier::REVERSED)
                && term
                    .terminal_view()
                    .contains("CREATE TRIGGER after_song_update")
        })
        .await
        .map_err(|e| e.terminal_view())
        .unwrap();
    tester
        .send_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()))
        .await;
    let (_, view) = tester.wait_for_completion().unwrap();
    let view = view.terminal_view();

    insta::with_settings!({
        snapshot_path => "../../../test/snapshots",
        filters => vec![(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}", "yyyy-mm-dd hh:mm:dd")]
    }, {
        insta::assert_snapshot!(view);
    });
}

fn setup<'a>(width: u16, height: u16) -> (UiTester<TuiApp<'a, TestBackend>, Buffer>, TempDir) {
    BroadcastWriter::disable();
    let (filter, reload_handle) =
        reload::Layer::new(Targets::default().with_target("slite", LevelFilter::INFO));
    Registry::default()
        .with(
            HierarchicalLayer::default()
                .with_writer(BroadcastWriter::default())
                .with_timestamps(false)
                .with_indent_lines(true)
                .with_level(false)
                .with_filter(filter),
        )
        .try_init()
        .ok();
    let mut conf = Conf::builder().file("./test/slite.toml").load().unwrap();
    let extensions = conf
        .extension_dir
        .map(read_extension_dir)
        .unwrap()
        .unwrap_or_default();
    let tempdir = tempfile::tempdir().unwrap();
    conf.target = Some(tempdir.path().join("test.db"));
    let ignore = conf.ignore.map(|i| i.0);
    let before_migration = conf.pre_migration.map(read_sql_files).unwrap_or_default();
    let after_migration = conf.post_migration.map(read_sql_files).unwrap_or_default();
    let config = slite::Config {
        extensions,
        ignore,
        before_migration,
        after_migration,
    };
    let app = TuiApp::<TestBackend>::new(
        MigratorFactory::new(conf.source.unwrap(), conf.target.unwrap(), config).unwrap(),
        reload_handle,
        Conf::default(),
    )
    .unwrap();
    let backend = TestBackend::new(width, height);
    let terminal = Terminal::new(backend).unwrap();
    let tester = UiTester::new_tui(app, terminal);
    (tester, tempdir)
}
