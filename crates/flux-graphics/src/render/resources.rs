//! ResourceRegistry: состояния ресурсов и bindless-слоты.
//!
//! Путь ассета: `Empty → Pending → Ready → Evicting → Empty`. Прокси-ресурс
//! позволяет рендерить до готовности загрузки; bindless-слоты
//! переиспользуются детерминированно (минимальный свободный индекс).

use std::collections::{BTreeMap, BTreeSet};

use super::super::rhi::{ImageId, RhiError};

/// Ключ ресурса (ассет/UUID, детерминированный).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceKey(pub u64);

/// Состояние ресурса.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceState {
    /// Не зарегистрирован.
    Empty,
    /// Загрузка запрошена, GPU-ресурса ещё нет.
    Pending,
    /// Ресурс готов к использованию.
    Ready,
    /// Вытесняется; использовать нельзя.
    Evicting,
}

/// Запись реестра.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceEntry {
    /// Состояние.
    pub state: ResourceState,
    /// Оценка размера, байт.
    pub bytes: u64,
    /// Bindless-слот.
    pub slot: u32,
    /// Готовый GPU-ресурс.
    pub image: Option<ImageId>,
    /// Прокси-ресурс на время `Pending`.
    pub proxy: Option<ImageId>,
}

/// Реестр ресурсов с bindless-слотами.
#[derive(Clone, Debug)]
pub struct ResourceRegistry {
    capacity: u32,
    next_slot: u32,
    free_slots: BTreeSet<u32>,
    entries: BTreeMap<ResourceKey, ResourceEntry>,
}

impl ResourceRegistry {
    /// Реестр на `capacity` bindless-слотов.
    #[must_use]
    pub fn new(capacity: u32) -> Self {
        Self {
            capacity,
            next_slot: 0,
            free_slots: BTreeSet::new(),
            entries: BTreeMap::new(),
        }
    }

    /// Ёмкость bindless-слотов.
    #[must_use]
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Число зарегистрированных ресурсов.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Пуст ли реестр.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Состояние ресурса.
    #[must_use]
    pub fn state(&self, key: ResourceKey) -> Option<ResourceState> {
        self.entries.get(&key).map(|entry| entry.state)
    }

    /// Запись ресурса.
    #[must_use]
    pub fn entry(&self, key: ResourceKey) -> Option<&ResourceEntry> {
        self.entries.get(&key)
    }

    /// Bindless-слот.
    #[must_use]
    pub fn slot(&self, key: ResourceKey) -> Option<u32> {
        self.entries.get(&key).map(|entry| entry.slot)
    }

    /// Готовое изображение.
    #[must_use]
    pub fn image(&self, key: ResourceKey) -> Option<ImageId> {
        self.entries.get(&key).and_then(|entry| entry.image)
    }

    /// Прокси-изображение.
    #[must_use]
    pub fn proxy(&self, key: ResourceKey) -> Option<ImageId> {
        self.entries.get(&key).and_then(|entry| entry.proxy)
    }

    /// Число ресурсов в состоянии.
    #[must_use]
    pub fn count(&self, state: ResourceState) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.state == state)
            .count()
    }

    /// Суммарный размер зарегистрированных ресурсов, байт.
    #[must_use]
    pub fn active_bytes(&self) -> u64 {
        self.entries.values().map(|entry| entry.bytes).sum()
    }

    /// Кандидаты на вытеснение: `Ready`, по возрастанию ключа.
    #[must_use]
    pub fn eviction_candidates(&self) -> Vec<ResourceKey> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.state == ResourceState::Ready)
            .map(|(key, _)| *key)
            .collect()
    }

    /// Регистрирует запрос загрузки и выдаёт bindless-слот.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — нулевой размер, дубликат ключа или
    /// слоты исчерпаны.
    pub fn insert_pending(&mut self, key: ResourceKey, bytes: u64) -> Result<u32, RhiError> {
        if bytes == 0 {
            return Err(RhiError::validation("resource size must be non-zero"));
        }
        if self.entries.contains_key(&key) {
            return Err(RhiError::validation("resource is already registered"));
        }
        let slot = if let Some(slot) = self.free_slots.pop_first() {
            slot
        } else if self.next_slot < self.capacity {
            let slot = self.next_slot;
            self.next_slot = self.next_slot.wrapping_add(1);
            slot
        } else {
            return Err(RhiError::validation("bindless slots exhausted"));
        };
        self.entries.insert(
            key,
            ResourceEntry {
                state: ResourceState::Pending,
                bytes,
                slot,
                image: None,
                proxy: None,
            },
        );
        Ok(slot)
    }

    /// Задаёт прокси-ресурс для `Pending`.
    ///
    /// # Errors
    ///
    /// [`RhiError::InvalidHandle`] — ресурс не зарегистрирован;
    /// [`RhiError::Validation`] — ресурс не в состоянии `Pending`.
    pub fn set_proxy(&mut self, key: ResourceKey, proxy: ImageId) -> Result<(), RhiError> {
        let entry = self.entries.get_mut(&key).ok_or(RhiError::InvalidHandle {
            kind: "resource",
            id: key.0,
        })?;
        if entry.state != ResourceState::Pending {
            return Err(RhiError::validation("proxy requires a pending resource"));
        }
        entry.proxy = Some(proxy);
        Ok(())
    }

    /// Переводит ресурс в `Ready`.
    ///
    /// # Errors
    ///
    /// [`RhiError::InvalidHandle`] — ресурс не зарегистрирован;
    /// [`RhiError::Validation`] — ресурс не в состоянии `Pending`.
    pub fn mark_ready(&mut self, key: ResourceKey, image: ImageId) -> Result<(), RhiError> {
        let entry = self.entries.get_mut(&key).ok_or(RhiError::InvalidHandle {
            kind: "resource",
            id: key.0,
        })?;
        if entry.state != ResourceState::Pending {
            return Err(RhiError::validation(
                "only a pending resource can become ready",
            ));
        }
        entry.state = ResourceState::Ready;
        entry.image = Some(image);
        entry.proxy = None;
        Ok(())
    }

    /// Снимает неудачную загрузку (`Pending → Empty`).
    ///
    /// # Errors
    ///
    /// [`RhiError::InvalidHandle`] — ресурс не зарегистрирован;
    /// [`RhiError::Validation`] — ресурс не в состоянии `Pending`.
    pub fn mark_failed(&mut self, key: ResourceKey) -> Result<(), RhiError> {
        let entry = self.entries.get(&key).ok_or(RhiError::InvalidHandle {
            kind: "resource",
            id: key.0,
        })?;
        if entry.state != ResourceState::Pending {
            return Err(RhiError::validation("only a pending resource can fail"));
        }
        let slot = entry.slot;
        self.entries.remove(&key);
        self.free_slots.insert(slot);
        Ok(())
    }

    /// Начинает вытеснение (`Ready → Evicting`).
    ///
    /// # Errors
    ///
    /// [`RhiError::InvalidHandle`] — ресурс не зарегистрирован;
    /// [`RhiError::Validation`] — ресурс не в состоянии `Ready`.
    pub fn begin_evict(&mut self, key: ResourceKey) -> Result<(), RhiError> {
        let entry = self.entries.get_mut(&key).ok_or(RhiError::InvalidHandle {
            kind: "resource",
            id: key.0,
        })?;
        if entry.state != ResourceState::Ready {
            return Err(RhiError::validation("only a ready resource can be evicted"));
        }
        entry.state = ResourceState::Evicting;
        Ok(())
    }

    /// Завершает вытеснение (`Evicting → Empty`) и возвращает изображение
    /// для уничтожения на GPU. Слот переиспользуется.
    ///
    /// # Errors
    ///
    /// [`RhiError::InvalidHandle`] — ресурс не зарегистрирован;
    /// [`RhiError::Validation`] — ресурс не в состоянии `Evicting`.
    pub fn finish_evict(&mut self, key: ResourceKey) -> Result<ImageId, RhiError> {
        let entry = self.entries.get(&key).ok_or(RhiError::InvalidHandle {
            kind: "resource",
            id: key.0,
        })?;
        if entry.state != ResourceState::Evicting {
            return Err(RhiError::validation("resource is not being evicted"));
        }
        let image = entry
            .image
            .ok_or_else(|| RhiError::validation("evicted resource has no image"))?;
        let slot = entry.slot;
        self.entries.remove(&key);
        self.free_slots.insert(slot);
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(value: u64) -> ResourceKey {
        ResourceKey(value)
    }

    #[test]
    fn lifecycle_transitions_and_slot_reuse() {
        let mut registry = ResourceRegistry::new(2);
        assert_eq!(registry.insert_pending(key(1), 1024).unwrap(), 0);
        assert_eq!(registry.state(key(1)), Some(ResourceState::Pending));
        registry.set_proxy(key(1), ImageId(9)).unwrap();
        assert_eq!(registry.proxy(key(1)), Some(ImageId(9)));
        registry.mark_ready(key(1), ImageId(10)).unwrap();
        assert_eq!(registry.state(key(1)), Some(ResourceState::Ready));
        assert_eq!(registry.image(key(1)), Some(ImageId(10)));
        assert_eq!(registry.proxy(key(1)), None);
        registry.begin_evict(key(1)).unwrap();
        assert_eq!(registry.state(key(1)), Some(ResourceState::Evicting));
        assert_eq!(registry.finish_evict(key(1)).unwrap(), ImageId(10));
        assert_eq!(registry.state(key(1)), None);
        assert_eq!(registry.insert_pending(key(2), 16).unwrap(), 0);
        assert_eq!(registry.insert_pending(key(3), 16).unwrap(), 1);
        assert!(registry.insert_pending(key(4), 16).is_err());
        assert_eq!(registry.active_bytes(), 32);
    }

    #[test]
    fn invalid_transitions_are_rejected() {
        let mut registry = ResourceRegistry::new(1);
        assert!(registry.mark_ready(key(1), ImageId(1)).is_err());
        registry.insert_pending(key(1), 8).unwrap();
        assert!(registry.begin_evict(key(1)).is_err());
        assert!(registry.set_proxy(key(1), ImageId(1)).is_ok());
        assert!(registry.mark_ready(key(1), ImageId(2)).is_ok());
        assert!(registry.mark_ready(key(1), ImageId(3)).is_err());
        assert!(registry.mark_failed(key(1)).is_err());
    }

    #[test]
    fn eviction_candidates_are_sorted() {
        let mut registry = ResourceRegistry::new(4);
        for value in [3, 1, 2] {
            registry.insert_pending(key(value), 8).unwrap();
            registry.mark_ready(key(value), ImageId(value)).unwrap();
        }
        assert_eq!(registry.eviction_candidates(), vec![key(1), key(2), key(3)]);
        assert_eq!(registry.count(ResourceState::Ready), 3);
    }

    #[test]
    fn registry_is_deterministic() {
        let mut first = ResourceRegistry::new(4);
        let mut second = ResourceRegistry::new(4);
        for registry in [&mut first, &mut second] {
            registry.insert_pending(key(7), 64).unwrap();
            registry.mark_ready(key(7), ImageId(7)).unwrap();
            registry.insert_pending(key(8), 32).unwrap();
        }
        assert_eq!(first.slot(key(7)), second.slot(key(7)));
        assert_eq!(first.active_bytes(), second.active_bytes());
    }
}
