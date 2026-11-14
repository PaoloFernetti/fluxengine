//! Крэш-сервисы: breadcrumbs, паник-хук и per-OS дампы (`crash-dump`).

use std::path::PathBuf;

pub use flux_core::crash::{Breadcrumbs, CrashHandler};

/// Конфигурация крэш-дампов.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrashConfig {
    /// Каталог дампов.
    pub dump_dir: PathBuf,
}

impl Default for CrashConfig {
    fn default() -> Self {
        Self {
            dump_dir: PathBuf::from("crash-dumps"),
        }
    }
}

impl CrashConfig {
    /// Конфигурация с каталогом.
    #[must_use]
    pub fn new(dump_dir: impl Into<PathBuf>) -> Self {
        Self {
            dump_dir: dump_dir.into(),
        }
    }
}

/// Устанавливает паник-хук с записью дампа и breadcrumbs.
pub fn install(config: &CrashConfig) {
    flux_core::crash::install_panic_hook(CrashHandler::new(config.dump_dir.clone()));
}

#[cfg(feature = "crash-dump")]
pub mod native {
    //! Per-OS дампы — отложены до отдельного ADR (I4).
    //!
    //! Windows `MiniDumpWriteDump` требует крейта `windows` и `unsafe`;
    //! Linux core pattern — системной настройки. Сейчас `install` пишет
    //! текстовый дамп (breadcrumbs + сообщение) через `flux-core`, а
    //! feature `crash-dump` остаётся точкой расширения без нативного кода.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_to_crash_dumps_dir() {
        assert_eq!(
            CrashConfig::default().dump_dir,
            PathBuf::from("crash-dumps")
        );
        assert_eq!(
            CrashConfig::new("logs/dumps").dump_dir,
            PathBuf::from("logs/dumps")
        );
    }
}
