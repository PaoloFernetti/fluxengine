//! Конфигурация: RON-секции, валидация, подписка на изменения.

use std::sync::Arc;

use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use thiserror::Error;

/// Ошибка конфигурации.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Ошибка разбора RON.
    #[error("config parse error: {0}")]
    Parse(String),
    /// Ошибка валидации.
    #[error("config validation failed: {0}")]
    Validation(String),
}

/// Секция конфигурации.
pub trait ConfigSection: DeserializeOwned + Default + Send + Sync + 'static {
    /// Проверяет инварианты секции.
    ///
    /// # Errors
    ///
    /// Возвращает текст ошибки, если значения недопустимы.
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

type Listener<T> = Arc<dyn Fn(&T) + Send + Sync>;

/// Хранилище типизированной секции с подпиской на изменения.
pub struct ConfigStore<T: ConfigSection> {
    value: RwLock<T>,
    listeners: RwLock<Vec<Listener<T>>>,
}

impl<T: ConfigSection> ConfigStore<T> {
    /// Создаёт хранилище со значениями по умолчанию.
    #[must_use]
    pub fn new() -> Self {
        Self {
            value: RwLock::new(T::default()),
            listeners: RwLock::new(Vec::new()),
        }
    }

    /// Загружает секцию из RON.
    ///
    /// # Errors
    ///
    /// Возвращает [`ConfigError`] при ошибке разбора или валидации.
    pub fn load_ron(&self, text: &str) -> Result<(), ConfigError> {
        let parsed: T =
            ron::from_str(text).map_err(|error| ConfigError::Parse(error.to_string()))?;
        parsed.validate().map_err(ConfigError::Validation)?;
        *self.value.write() = parsed;
        self.notify();
        Ok(())
    }

    /// Текущее значение (копия).
    #[must_use]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.value.read().clone()
    }

    /// Заменяет значение после валидации.
    ///
    /// # Errors
    ///
    /// Возвращает [`ConfigError::Validation`], если значение недопустимо.
    pub fn set(&self, value: T) -> Result<(), ConfigError> {
        value.validate().map_err(ConfigError::Validation)?;
        *self.value.write() = value;
        self.notify();
        Ok(())
    }

    /// Подписывается на изменения.
    pub fn on_change<F>(&self, listener: F)
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.listeners.write().push(Arc::new(listener));
    }

    fn notify(&self) {
        let value = self.value.read();
        for listener in self.listeners.read().iter() {
            listener(&value);
        }
    }
}

impl<T: ConfigSection> Default for ConfigStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone, Debug, Default, Deserialize)]
    struct TestConfig {
        value: u32,
    }

    impl ConfigSection for TestConfig {
        fn validate(&self) -> Result<(), String> {
            if self.value <= 10 {
                Ok(())
            } else {
                Err("value must be <= 10".to_string())
            }
        }
    }

    #[test]
    fn loads_valid_ron() {
        let store = ConfigStore::<TestConfig>::new();
        store.load_ron("(value: 5)").unwrap();
        assert_eq!(store.get().value, 5);
    }

    #[test]
    fn rejects_invalid_values() {
        let store = ConfigStore::<TestConfig>::new();
        assert!(store.load_ron("(value: 50)").is_err());
        assert!(store.set(TestConfig { value: 50 }).is_err());
        assert_eq!(store.get().value, 0);
    }

    #[test]
    fn notifies_listeners() {
        let store = ConfigStore::<TestConfig>::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        store.on_change(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        store.set(TestConfig { value: 3 }).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
