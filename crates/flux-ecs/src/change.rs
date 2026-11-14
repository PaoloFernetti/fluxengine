//! Change detection: `ComponentTick { added, changed }` на `SimTick`.
//!
//! Метки ставятся только мутациями мира: `insert` (новый компонент),
//! `insert` поверх существующего (только `changed`), `spawn` (оба).
//! Чтение меток не меняет — поэтому `Added<T>` и `Changed<T>` не дают
//! ложных срабатываний между тиками.
//!
//! Порог хранится у читателя: [`LastRun`] (или `SimTick` из прошлого запуска
//! системы). Тик `ZERO` считается «до начала симуляции»: планировщик вызывает
//! [`World::advance_tick`](crate::world::World::advance_tick) перед стадией.

use flux_core::time::SimTick;

/// Метки последнего добавления и изменения компонента.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ComponentTick {
    /// Тик, на котором компонент появился у сущности.
    pub added: SimTick,
    /// Тик последней записи значения.
    pub changed: SimTick,
}

impl ComponentTick {
    /// Новая метка: компонент добавлен и изменён на `tick`.
    #[must_use]
    pub const fn new(tick: SimTick) -> Self {
        Self {
            added: tick,
            changed: tick,
        }
    }

    /// Метка замены значения: `added` сохраняется, `changed` обновляется.
    #[must_use]
    pub const fn replaced(self, tick: SimTick) -> Self {
        Self {
            added: self.added,
            changed: tick,
        }
    }

    /// Компонент добавлен позже `last_run`.
    #[must_use]
    pub fn is_added_since(self, last_run: SimTick) -> bool {
        self.added > last_run
    }

    /// Компонент изменён позже `last_run`.
    #[must_use]
    pub fn is_changed_since(self, last_run: SimTick) -> bool {
        self.changed > last_run
    }
}

/// Порог чтения для `Added`/`Changed`: тик последнего запуска системы.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LastRun(SimTick);

impl LastRun {
    /// Порог по тику.
    #[must_use]
    pub const fn new(tick: SimTick) -> Self {
        Self(tick)
    }

    /// Значение порога.
    #[must_use]
    pub const fn get(self) -> SimTick {
        self.0
    }

    /// Сдвигает порог на текущий тик.
    pub fn set(&mut self, tick: SimTick) {
        self.0 = tick;
    }

    /// Компонент добавлен после порога.
    #[must_use]
    pub fn is_added(self, tick: ComponentTick) -> bool {
        tick.is_added_since(self.0)
    }

    /// Компонент изменён после порога.
    #[must_use]
    pub fn is_changed(self, tick: ComponentTick) -> bool {
        tick.is_changed_since(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_keeps_added_tick() {
        let added = ComponentTick::new(SimTick::ZERO);
        let mut tick = SimTick::ZERO;
        tick.advance(5);
        let replaced = added.replaced(tick);
        assert_eq!(replaced.added, SimTick::ZERO);
        assert_eq!(replaced.changed, tick);
    }

    #[test]
    fn last_run_separates_ticks() {
        let mut tick = SimTick::ZERO;
        tick.advance(3);
        let component = ComponentTick::new(tick);

        let mut last_run = LastRun::default();
        assert!(last_run.is_added(component));
        assert!(last_run.is_changed(component));

        last_run.set(tick);
        assert!(!last_run.is_added(component));
        assert!(!last_run.is_changed(component));

        let mut next = tick;
        next.advance(1);
        let replaced = component.replaced(next);
        assert!(!last_run.is_added(replaced));
        assert!(last_run.is_changed(replaced));
    }
}
