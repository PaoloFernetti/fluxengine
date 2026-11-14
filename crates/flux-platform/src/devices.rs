//! Устройства ввода: геймпады и hot-plug.
//!
//! Реестр не зависит от `gilrs`: адаптер (`devices` feature) подаёт события
//! [`DeviceEvent`], а headless-профиль может делать это вручную (ADR-0033).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Подключённый геймпад.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GamepadInfo {
    /// Идентификатор в рамках сессии.
    pub id: u16,
    /// Имя устройства.
    pub name: String,
    /// Vendor ID.
    pub vendor: u16,
    /// Product ID.
    pub product: u16,
}

/// Событие hot-plug.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceEvent {
    /// Устройство подключено.
    Connected(GamepadInfo),
    /// Устройство отключено.
    Disconnected(u16),
}

/// Реестр подключённых геймпадов (детерминированный порядок по `id`).
#[derive(Clone, Debug, Default)]
pub struct DeviceRegistry {
    connected: BTreeMap<u16, GamepadInfo>,
}

impl DeviceRegistry {
    /// Пустой реестр.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Применяет событие hot-plug.
    pub fn apply(&mut self, event: &DeviceEvent) {
        match event {
            DeviceEvent::Connected(info) => {
                self.connected.insert(info.id, info.clone());
            }
            DeviceEvent::Disconnected(id) => {
                self.connected.remove(id);
            }
        }
    }

    /// Подключённые устройства в порядке `id`.
    pub fn connected(&self) -> impl Iterator<Item = &GamepadInfo> {
        self.connected.values()
    }

    /// Подключён ли геймпад.
    #[must_use]
    pub fn is_connected(&self, id: u16) -> bool {
        self.connected.contains_key(&id)
    }

    /// Число подключённых устройств.
    #[must_use]
    pub fn len(&self) -> usize {
        self.connected.len()
    }

    /// Нет ли устройств.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.connected.is_empty()
    }
}

#[cfg(feature = "devices")]
pub mod gilrs_backend {
    //! Адаптер `gilrs`: hot-plug и опрос событий (feature `devices`, I4).
    //!
    //! Кнопки/оси пока не транслируются в `InputState`: это следующий шаг
    //! после hot-plug. Реестр устройств уже детерминирован по `id`.

    use super::{DeviceEvent, DeviceRegistry, GamepadInfo};

    /// Идентификатор `gilrs` → `u16` (насыщение на границе).
    fn gamepad_id(id: gilrs::GamepadId) -> u16 {
        u16::try_from(usize::from(id)).unwrap_or(u16::MAX)
    }

    /// Опрашивает `gilrs` и возвращает hot-plug события.
    #[must_use]
    pub fn poll(gilrs: &mut gilrs::Gilrs) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        while let Some(event) = gilrs.next_event() {
            let id = gamepad_id(event.id);
            match event.event {
                gilrs::EventType::Connected => {
                    let gamepad = gilrs.gamepad(event.id);
                    events.push(DeviceEvent::Connected(GamepadInfo {
                        id,
                        name: gamepad.name().to_owned(),
                        vendor: 0,
                        product: 0,
                    }));
                }
                gilrs::EventType::Disconnected => {
                    events.push(DeviceEvent::Disconnected(id));
                }
                _ => {}
            }
        }
        events
    }

    /// Опрашивает `gilrs` и применяет hot-plug к реестру.
    pub fn poll_registry(gilrs: &mut gilrs::Gilrs, registry: &mut DeviceRegistry) {
        for event in poll(gilrs) {
            registry.apply(&event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(id: u16) -> GamepadInfo {
        GamepadInfo {
            id,
            name: format!("pad-{id}"),
            vendor: 0x1234,
            product: 0x5678,
        }
    }

    #[test]
    fn hot_plug_updates_registry() {
        let mut registry = DeviceRegistry::new();
        registry.apply(&DeviceEvent::Connected(pad(2)));
        registry.apply(&DeviceEvent::Connected(pad(1)));
        assert_eq!(registry.len(), 2);
        assert!(registry.is_connected(1));
        // Порядок детерминирован: по id, а не по порядку подключения.
        let ids: Vec<u16> = registry.connected().map(|info| info.id).collect();
        assert_eq!(ids, vec![1, 2]);

        registry.apply(&DeviceEvent::Disconnected(1));
        assert!(!registry.is_connected(1));
        assert_eq!(registry.len(), 1);
    }
}
