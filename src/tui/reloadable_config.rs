use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use arc_swap::ArcSwap;
use confique::Config;
use notify::RecommendedWatcher;
use notify_debouncer_mini::{new_debouncer, DebouncedEvent, Debouncer};

pub trait ConfigHandler<T: Config + Send + Sync + 'static>: Send + 'static {
    fn on_update(
        &mut self,
        previous_config: Arc<T>,
        new_config: Arc<T>,
        events: Vec<DebouncedEvent>,
    );
    fn create_config(&self, path: &Path) -> T;
    fn watch_paths(&self, path: &Path) -> Vec<PathBuf>;
}

pub struct ReloadableConfig<T: Config + Send + Sync + 'static> {
    current_config: Arc<ArcSwap<T>>,
    cached_config: Arc<ArcSwap<T>>,
    _debouncer: Debouncer<RecommendedWatcher>,
}

impl<T: Config + Send + Sync + 'static> ReloadableConfig<T> {
    pub fn new(path: PathBuf, mut handler: impl ConfigHandler<T>) -> Self {
        let paths = handler.watch_paths(&path);
        let cached_config = Arc::new(ArcSwap::new(Arc::new(handler.create_config(&path))));
        let current_config = Arc::new(ArcSwap::new(Arc::new(handler.create_config(&path))));

        let current_config_ = current_config.clone();
        let mut debouncer = new_debouncer(Duration::from_millis(250), None, move |events| {
            if let Ok(events) = events {
                let new_config = Arc::new(handler.create_config(&path));
                let previous_config = current_config_.load_full();
                current_config_.store(new_config.clone());
                handler.on_update(previous_config, new_config, events);
            }
        })
        .unwrap();

        for path in paths {
            if path.exists() {
                debouncer
                    .watcher()
                    .watch(&path, notify::RecursiveMode::Recursive)
                    .unwrap();
            }
        }

        Self {
            _debouncer: debouncer,
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
}
