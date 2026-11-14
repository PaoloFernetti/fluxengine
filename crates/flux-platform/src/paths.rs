//! Пути проекта: ассеты, сейвы, логи, крэш-дампы.

use std::path::{Path, PathBuf};

/// Каталоги проекта.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectPaths {
    /// Корень проекта.
    pub root: PathBuf,
    /// Ассеты.
    pub assets: PathBuf,
    /// Сохранения.
    pub saves: PathBuf,
    /// Логи.
    pub logs: PathBuf,
    /// Крэш-дампы.
    pub crash_dumps: PathBuf,
}

impl ProjectPaths {
    /// Пути относительно корня.
    #[must_use]
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            assets: root.join("assets"),
            saves: root.join("saves"),
            logs: root.join("logs"),
            crash_dumps: root.join("crash-dumps"),
            root,
        }
    }

    /// Все каталоги в каноническом порядке.
    #[must_use]
    pub fn directories(&self) -> [&Path; 5] {
        [
            self.root.as_path(),
            self.assets.as_path(),
            self.saves.as_path(),
            self.logs.as_path(),
            self.crash_dumps.as_path(),
        ]
    }

    /// Создаёт каталоги (идемпотентно).
    pub fn ensure(&self) -> std::io::Result<()> {
        for directory in self.directories() {
            std::fs::create_dir_all(directory)?;
        }
        Ok(())
    }

    /// Путь к файлу сохранения.
    #[must_use]
    pub fn save_file(&self, name: &str) -> PathBuf {
        self.saves.join(name)
    }

    /// Путь к файлу лога.
    #[must_use]
    pub fn log_file(&self, name: &str) -> PathBuf {
        self.logs.join(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_derived_from_root() {
        let paths = ProjectPaths::from_root("game");
        assert_eq!(paths.assets, PathBuf::from("game/assets"));
        assert_eq!(paths.saves, PathBuf::from("game/saves"));
        assert_eq!(paths.logs, PathBuf::from("game/logs"));
        assert_eq!(paths.crash_dumps, PathBuf::from("game/crash-dumps"));
        assert_eq!(
            paths.save_file("slot1.ron"),
            PathBuf::from("game/saves/slot1.ron")
        );
        assert_eq!(
            paths.log_file("run.log"),
            PathBuf::from("game/logs/run.log")
        );
        assert_eq!(paths.directories().len(), 5);
    }
}
