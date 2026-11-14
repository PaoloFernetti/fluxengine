//! Буфер обмена: контракт и in-memory реализация для headless/тестов.

/// Буфер обмена.
pub trait Clipboard: Send + Sync {
    /// Текст из буфера.
    fn get_text(&self) -> Option<String>;
    /// Записывает текст.
    fn set_text(&mut self, text: &str);
    /// Очищает буфер.
    fn clear(&mut self);
}

/// Буфер в памяти (headless-профиль, тесты).
#[derive(Clone, Debug, Default)]
pub struct MemoryClipboard {
    text: Option<String>,
}

impl MemoryClipboard {
    /// Пустой буфер.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Текст без копирования.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

impl Clipboard for MemoryClipboard {
    fn get_text(&self) -> Option<String> {
        self.text.clone()
    }

    fn set_text(&mut self, text: &str) {
        self.text = Some(text.to_owned());
    }

    fn clear(&mut self) {
        self.text = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_clipboard_roundtrips_text() {
        let mut clipboard = MemoryClipboard::new();
        assert_eq!(clipboard.get_text(), None);
        clipboard.set_text("flux");
        assert_eq!(clipboard.as_str(), Some("flux"));
        assert_eq!(clipboard.get_text().as_deref(), Some("flux"));
        clipboard.clear();
        assert_eq!(clipboard.get_text(), None);
    }
}
