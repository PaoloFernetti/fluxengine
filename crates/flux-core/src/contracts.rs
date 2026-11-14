//! Контракты подсистем: подсистемы видят друг друга только через trait-ы.
//!
//! Реализации живут в соответствующих крейтах; ядро не зависит от них.

/// Платформенный слой.
pub trait Platform: Send + Sync {
    /// Имя платформы.
    fn name(&self) -> &'static str;

    /// Число фоновых воркеров (N−1 от доступного параллелизма).
    fn worker_threads(&self) -> usize {
        crate::platform::available_parallelism().saturating_sub(1)
    }
}

/// Рендер-бэкенд (реализация — `flux-graphics`, этап 4).
pub trait Renderer: Send + Sync {
    /// Имя бэкенда (`vulkan`, `dx12`).
    fn backend_name(&self) -> &'static str;
}

/// Аудио-устройство (реализация — `flux-audio`, этап 6).
pub trait AudioDevice: Send + Sync {
    /// Частота дискретизации.
    fn sample_rate(&self) -> u32;
}

/// Сетевой транспорт (реализация — `flux-net`, этап 5).
pub trait NetBackend: Send + Sync {
    /// Имя транспорта (`loopback`, `udp`).
    fn name(&self) -> &'static str;
}

/// Скриптовая VM (реализация — `flux-script`, этап 6).
pub trait ScriptVM: Send + Sync {
    /// Имя VM.
    fn name(&self) -> &'static str;
}

/// Поставщик релевантности (контракт разрывает цикл scene ↔ net).
pub trait RelevanceProvider: Send + Sync {
    /// Ярус релевантности сущности (`0..=3`).
    fn relevance_tier(&self, entity: u64) -> u8;
}
