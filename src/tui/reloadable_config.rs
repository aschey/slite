use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arc_swap::ArcSwap;
use confique::Config;
use elm_ui::Command;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebouncedEvent, Debouncer, new_debouncer};
use tokio::sync::mpsc;
use tracing::error;

pub trait ConfigHandler<T: Config + Send + Sync + 'static>: Send + 'static {
    fn on_update(
        &mut self,
        previous_config: Arc<T>,
        new_config: Arc<T>,
        events: Vec<DebouncedEvent>,
    ) -> Result<(), mpsc::error::SendError<Command>>;
    fn create_config(&self, path: &Path) -> T;
    fn watch_paths(&self, path: &Path) -> Vec<PathBuf>;
}

#[derive(Clone)]
pub struct ReloadableConfig<T: Config + Debug + Send + Sync + 'static> {
    current_config: Arc<ArcSwap<T>>,
    cached_config: Arc<ArcSwap<T>>,
    debouncer: Arc<Mutex<Debouncer<RecommendedWatcher>>>,
}

impl<T: Config + Debug + Send + Sync + 'static> Debug for ReloadableConfig<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReloadableConfig")
            .field("current_config", &self.current_config)
            .field("cached_config", &self.cached_config)
            .field("debouncer", &"<debouncer>")
            .finish()
    }
}

impl<T: Config + Debug + Send + Sync + 'static> ReloadableConfig<T> {
    pub fn new(path: PathBuf, mut handler: impl ConfigHandler<T>) -> Self {
        let paths = handler.watch_paths(&path);
        let cached_config = Arc::new(ArcSwap::new(Arc::new(handler.create_config(&path))));
        let current_config = Arc::new(ArcSwap::new(Arc::new(handler.create_config(&path))));

        let current_config_ = current_config.clone();
        let mut debouncer = new_debouncer(Duration::from_millis(250), move |events| {
            if let Ok(events) = events {
                let new_config = Arc::new(handler.create_config(&path));
                let previous_config = current_config_.load_full();
                current_config_.store(new_config.clone());
                if let Err(e) = handler.on_update(previous_config, new_config, events) {
                    error!("{e}");
                }
            }
        })
        .unwrap();

        for path in paths {
            if path.exists()
                && let Err(e) = debouncer.watcher().watch(&path, RecursiveMode::Recursive) {
                    error!("{e}");
                }
        }

        Self {
            debouncer: Arc::new(Mutex::new(debouncer)),
            cached_config,
            current_config,
        }
    }

    pub fn snapshot(&self) -> Arc<T> {
        self.cached_config.load_full()
    }

    pub fn load(&self) -> Arc<T> {
        let current = self.current_config.load_full();
        self.cached_config.store(current.clone());
        current
    }

    pub fn switch_path(&mut self, old_path: Option<&Path>, new_path: Option<&Path>) {
        if let Some(old_path) = old_path
            && old_path.exists() {
                self.debouncer
                    .lock()
                    .unwrap()
                    .watcher()
                    .unwatch(old_path)
                    .unwrap();
            }

        if let Some(new_path) = new_path
            && new_path.exists()
                && let Err(e) = self
                    .debouncer
                    .lock()
                    .unwrap()
                    .watcher()
                    .watch(new_path, RecursiveMode::Recursive)
                {
                    error!("{e}");
                }
    }
}
