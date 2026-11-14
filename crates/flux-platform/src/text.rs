//! Текст и IME: композиция не попадает в committed-поток до подтверждения.

use serde::{Deserialize, Serialize};

/// Событие IME-композиции.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImeEvent {
    /// Начало композиции.
    Start,
    /// Обновление композиции.
    Update {
        /// Текущий текст композиции.
        text: String,
        /// Выделение (начало, конец) в байтах.
        cursor: Option<(usize, usize)>,
    },
    /// Конец композиции: текст фиксируется.
    End,
}

/// Текстовое событие.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEvent {
    /// IME-композиция.
    Ime(ImeEvent),
    /// Готовый текст (вставка, ввод без IME).
    Commit(String),
}

/// Аккумулятор текстового ввода.
#[derive(Clone, Debug, Default)]
pub struct TextInput {
    composition: Option<String>,
    committed: String,
}

impl TextInput {
    /// Пустой аккумулятор.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Применяет событие.
    pub fn apply(&mut self, event: TextEvent) {
        match event {
            TextEvent::Ime(ImeEvent::Start) => self.composition = Some(String::new()),
            TextEvent::Ime(ImeEvent::Update { text, .. }) => self.composition = Some(text),
            TextEvent::Ime(ImeEvent::End) => {
                if let Some(text) = self.composition.take() {
                    self.committed.push_str(&text);
                }
            }
            TextEvent::Commit(text) => self.committed.push_str(&text),
        }
    }

    /// Текущая композиция.
    #[must_use]
    pub fn composition(&self) -> Option<&str> {
        self.composition.as_deref()
    }

    /// Идёт ли композиция.
    #[must_use]
    pub fn is_composing(&self) -> bool {
        self.composition.is_some()
    }

    /// Подтверждённый текст.
    #[must_use]
    pub fn committed(&self) -> &str {
        &self.committed
    }

    /// Забирает подтверждённый текст, очищая аккумулятор.
    pub fn take_committed(&mut self) -> String {
        std::mem::take(&mut self.committed)
    }

    /// Отменяет композицию без фиксации.
    pub fn cancel_composition(&mut self) {
        self.composition = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ime_commits_only_on_end() {
        let mut input = TextInput::new();
        input.apply(TextEvent::Ime(ImeEvent::Start));
        input.apply(TextEvent::Ime(ImeEvent::Update {
            text: "に".to_owned(),
            cursor: None,
        }));
        assert!(input.is_composing());
        assert_eq!(input.committed(), "");

        input.apply(TextEvent::Ime(ImeEvent::Update {
            text: "にほ".to_owned(),
            cursor: None,
        }));
        input.apply(TextEvent::Ime(ImeEvent::End));
        assert!(!input.is_composing());
        assert_eq!(input.committed(), "にほ");
        assert_eq!(input.take_committed(), "にほ");
        assert_eq!(input.committed(), "");
    }

    #[test]
    fn cancelled_composition_is_dropped() {
        let mut input = TextInput::new();
        input.apply(TextEvent::Ime(ImeEvent::Start));
        input.apply(TextEvent::Ime(ImeEvent::Update {
            text: "abc".to_owned(),
            cursor: None,
        }));
        input.cancel_composition();
        assert_eq!(input.committed(), "");
    }

    #[test]
    fn direct_commit_appends() {
        let mut input = TextInput::new();
        input.apply(TextEvent::Commit("ab".to_owned()));
        input.apply(TextEvent::Commit("cd".to_owned()));
        assert_eq!(input.committed(), "abcd");
    }
}
