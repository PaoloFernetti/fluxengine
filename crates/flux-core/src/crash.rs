//! Краш-контракт: breadcrumbs, паник-хук, дамп.
//!
//! Реализация per-OS живёт в `flux-platform/crash`; здесь — контракт и
//! кроссплатформенный минимум.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use parking_lot::Mutex;

const DEFAULT_BREADCRUMB_CAPACITY: usize = 64;

/// Кольцо breadcrumbs.
#[derive(Debug)]
pub struct Breadcrumbs {
    entries: Mutex<VecDeque<String>>,
    capacity: usize,
}

impl Breadcrumbs {
    /// Создаёт кольцо на `capacity` записей.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            capacity: capacity.max(1),
        }
    }

    /// Добавляет запись, вытесняя самую старую.
    pub fn push(&self, message: impl Into<String>) {
        let mut entries = self.entries.lock();
        if entries.len() == self.capacity {
            entries.pop_front();
        }
        entries.push_back(message.into());
    }

    /// Снимок в порядке добавления.
    #[must_use]
    pub fn snapshot(&self) -> Vec<String> {
        self.entries.lock().iter().cloned().collect()
    }

    /// Число записей.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    /// Пусто ли кольцо.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Очищает.
    pub fn clear(&self) {
        self.entries.lock().clear();
    }
}

static GLOBAL_BREADCRUMBS: OnceLock<Breadcrumbs> = OnceLock::new();

/// Глобальные breadcrumbs.
pub fn breadcrumbs() -> &'static Breadcrumbs {
    GLOBAL_BREADCRUMBS.get_or_init(|| Breadcrumbs::new(DEFAULT_BREADCRUMB_CAPACITY))
}

/// Обработчик крашей.
#[derive(Clone, Debug)]
pub struct CrashHandler {
    dump_dir: PathBuf,
}

impl CrashHandler {
    /// Создаёт обработчик с каталогом дампов.
    #[must_use]
    pub fn new(dump_dir: impl Into<PathBuf>) -> Self {
        Self {
            dump_dir: dump_dir.into(),
        }
    }

    /// Каталог дампов.
    #[must_use]
    pub fn dump_dir(&self) -> &Path {
        &self.dump_dir
    }

    /// Записывает текстовый дамп с breadcrumbs.
    ///
    /// # Errors
    ///
    /// Возвращает ошибку ввода-вывода.
    pub fn write_dump(&self, message: &str) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dump_dir)?;
        let path = self
            .dump_dir
            .join(format!("crash-{}.txt", std::process::id()));
        let mut contents = String::new();
        contents.push_str("FluxEngine crash dump\n");
        contents.push_str("=====================\n");
        contents.push_str(message);
        contents.push_str("\n\nbreadcrumbs:\n");
        for crumb in breadcrumbs().snapshot() {
            contents.push_str("  - ");
            contents.push_str(&crumb);
            contents.push('\n');
        }
        std::fs::write(&path, contents)?;
        Ok(path)
    }
}

/// Устанавливает паник-хук: лог, breadcrumbs, дамп, затем штатный хук.
pub fn install_panic_hook(handler: CrashHandler) {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        let message = format!("{info}");
        tracing::error!(panic = %message, "panic");
        for crumb in breadcrumbs().snapshot() {
            eprintln!("  breadcrumb: {crumb}");
        }
        if let Err(error) = handler.write_dump(&message) {
            eprintln!("failed to write crash dump: {error}");
        }
        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumbs_drop_oldest() {
        let breadcrumbs = Breadcrumbs::new(3);
        for index in 0..5 {
            breadcrumbs.push(format!("event-{index}"));
        }
        assert_eq!(breadcrumbs.len(), 3);
        assert_eq!(
            breadcrumbs.snapshot(),
            vec!["event-2", "event-3", "event-4"]
        );
    }

    #[test]
    fn crash_dump_contains_breadcrumbs() {
        let dir = std::env::temp_dir().join(format!("flux-crash-test-{}", std::process::id()));
        let handler = CrashHandler::new(&dir);
        breadcrumbs().push("before crash");
        let path = handler.write_dump("boom").unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("boom"));
        assert!(contents.contains("before crash"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
