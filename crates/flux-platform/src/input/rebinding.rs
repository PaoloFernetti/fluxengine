//! Rebinding: сериализуемый профиль биндингов, захват события, конфликты.
//!
//! Профиль — данные, а не код: RON-roundtrip сохраняет его между запусками.
//! Конфликт — один источник, назначенный разным действиям одного игрока
//! в одном контексте. Одинаковый источник в разных контекстах конфликтом
//! не является (верхний контекст поглощает действие, ADR-0031).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::mapping::{ActionKind, AxisComponent, BindingSource, PlayerId};
use super::raw::{RawEvent, RawEventKind};

/// Запись профиля: действие + источник + контекст.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileBinding {
    /// UUID действия (стабильный контракт, ADR-0031).
    pub action: Uuid,
    /// Имя действия для читаемости профиля.
    pub action_name: String,
    /// Тип значения действия.
    pub kind: ActionKind,
    /// Источник.
    pub source: BindingSource,
    /// Игрок.
    pub player: PlayerId,
    /// Контекст (`""` — глобальный).
    pub context: String,
    /// Вклад в значение.
    pub component: AxisComponent,
}

impl ProfileBinding {
    /// Создаёт запись профиля.
    #[must_use]
    pub fn new(
        action: Uuid,
        action_name: impl Into<String>,
        kind: ActionKind,
        source: BindingSource,
        player: PlayerId,
    ) -> Self {
        Self {
            action,
            action_name: action_name.into(),
            kind,
            source,
            player,
            context: String::new(),
            component: AxisComponent::button(),
        }
    }

    /// Биндинг в контексте.
    #[must_use]
    pub fn in_context(mut self, context: impl Into<String>) -> Self {
        self.context = context.into();
        self
    }

    /// Вклад в ось.
    #[must_use]
    pub fn with_component(mut self, component: AxisComponent) -> Self {
        self.component = component;
        self
    }
}

/// Конфликт биндингов.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    /// Игрок.
    pub player: PlayerId,
    /// Контекст.
    pub context: String,
    /// Источник, назначенный нескольким действиям.
    pub source: BindingSource,
    /// Действия (отсортированы по UUID).
    pub actions: Vec<Uuid>,
}

/// Профиль биндингов, сохраняемый в RON.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BindingProfile {
    /// Записи профиля.
    pub entries: Vec<ProfileBinding>,
}

impl BindingProfile {
    /// Пустой профиль.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавляет запись.
    pub fn bind(&mut self, entry: ProfileBinding) {
        self.entries.push(entry);
    }

    /// Записи (только чтение).
    #[must_use]
    pub fn entries(&self) -> &[ProfileBinding] {
        &self.entries
    }

    /// Захват источника из следующего raw-события.
    ///
    /// Автоповтор ОС и отпускание клавиш игнорируются: пользователь должен
    /// назначить именно новое нажатие.
    #[must_use]
    pub fn capture(event: &RawEvent) -> Option<BindingSource> {
        match &event.kind {
            RawEventKind::Key {
                scancode,
                pressed: true,
                repeat: false,
            } => Some(BindingSource::Key(*scancode)),
            RawEventKind::MouseButton {
                button,
                pressed: true,
            } => Some(BindingSource::Mouse(*button)),
            _ => None,
        }
    }

    /// Переназначает источник существующего действия.
    ///
    /// Возвращает `true`, если запись найдена. Новые действия добавляются
    /// через [`BindingProfile::bind`].
    pub fn rebind(
        &mut self,
        action: Uuid,
        player: PlayerId,
        context: &str,
        source: BindingSource,
    ) -> bool {
        for entry in &mut self.entries {
            if entry.action == action && entry.player == player && entry.context == context {
                entry.source = source;
                return true;
            }
        }
        false
    }

    /// Конфликты: источник → несколько действий у одного игрока в контексте.
    ///
    /// Результат отсортирован: игрок, контекст, источник.
    #[must_use]
    pub fn detect_conflicts(&self) -> Vec<Conflict> {
        let mut grouped: BTreeMap<(PlayerId, String, BindingSource), BTreeSet<Uuid>> =
            BTreeMap::new();
        for entry in &self.entries {
            grouped
                .entry((entry.player, entry.context.clone(), entry.source))
                .or_default()
                .insert(entry.action);
        }
        grouped
            .into_iter()
            .filter(|(_, actions)| actions.len() > 1)
            .map(|((player, context, source), actions)| Conflict {
                player,
                context,
                source,
                actions: actions.into_iter().collect(),
            })
            .collect()
    }

    /// Профиль в RON.
    pub fn to_ron(&self) -> Result<String, ron::Error> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
    }

    /// Профиль из RON.
    pub fn from_ron(text: &str) -> Result<Self, ron::de::SpannedError> {
        ron::from_str(text)
    }
}

/// Конфликты профиля (свободная функция для API ADR-0031).
#[must_use]
pub fn detect_conflicts(profile: &BindingProfile) -> Vec<Conflict> {
    profile.detect_conflicts()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::input::mapping::InputAction;
    use crate::input::raw::Scancode;

    const MOVE: InputAction = InputAction::new(
        "move",
        Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0400),
        ActionKind::Axis2,
    );
    const JUMP: InputAction = InputAction::new(
        "jump",
        Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0401),
        ActionKind::Button,
    );

    fn key_press(scancode: Scancode) -> RawEvent {
        RawEvent::new(
            Duration::from_millis(1),
            RawEventKind::Key {
                scancode,
                pressed: true,
                repeat: false,
            },
        )
    }

    #[test]
    fn profile_roundtrips_through_ron() {
        let mut profile = BindingProfile::new();
        profile.bind(ProfileBinding::new(
            MOVE.id,
            MOVE.name,
            MOVE.kind,
            BindingSource::Key(Scancode::W),
            PlayerId(0),
        ));
        profile.bind(
            ProfileBinding::new(
                JUMP.id,
                JUMP.name,
                JUMP.kind,
                BindingSource::Key(Scancode::SPACE),
                PlayerId(0),
            )
            .in_context("gameplay"),
        );

        let text = profile.to_ron().unwrap();
        let restored = BindingProfile::from_ron(&text).unwrap();
        assert_eq!(profile, restored);
    }

    #[test]
    fn conflict_is_detected_per_context() {
        let mut profile = BindingProfile::new();
        profile.bind(ProfileBinding::new(
            MOVE.id,
            MOVE.name,
            MOVE.kind,
            BindingSource::Key(Scancode::SPACE),
            PlayerId(0),
        ));
        profile.bind(ProfileBinding::new(
            JUMP.id,
            JUMP.name,
            JUMP.kind,
            BindingSource::Key(Scancode::SPACE),
            PlayerId(0),
        ));
        // Тот же источник в другом контексте — не конфликт.
        profile.bind(
            ProfileBinding::new(
                JUMP.id,
                JUMP.name,
                JUMP.kind,
                BindingSource::Key(Scancode::SPACE),
                PlayerId(0),
            )
            .in_context("ui"),
        );

        let conflicts = detect_conflicts(&profile);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].source, BindingSource::Key(Scancode::SPACE));
        assert_eq!(conflicts[0].actions, vec![MOVE.id, JUMP.id]);
    }

    #[test]
    fn capture_ignores_repeat_and_release() {
        assert_eq!(
            BindingProfile::capture(&key_press(Scancode::W)),
            Some(BindingSource::Key(Scancode::W))
        );
        let repeat = RawEvent::new(
            Duration::ZERO,
            RawEventKind::Key {
                scancode: Scancode::W,
                pressed: true,
                repeat: true,
            },
        );
        assert_eq!(BindingProfile::capture(&repeat), None);
        let release = RawEvent::new(
            Duration::ZERO,
            RawEventKind::Key {
                scancode: Scancode::W,
                pressed: false,
                repeat: false,
            },
        );
        assert_eq!(BindingProfile::capture(&release), None);
    }

    #[test]
    fn rebind_changes_existing_entry() {
        let mut profile = BindingProfile::new();
        profile.bind(ProfileBinding::new(
            JUMP.id,
            JUMP.name,
            JUMP.kind,
            BindingSource::Key(Scancode::SPACE),
            PlayerId(0),
        ));
        assert!(profile.rebind(JUMP.id, PlayerId(0), "", BindingSource::Key(Scancode::F1)));
        assert_eq!(
            profile.entries()[0].source,
            BindingSource::Key(Scancode::F1)
        );
        assert!(!profile.rebind(JUMP.id, PlayerId(1), "", BindingSource::Key(Scancode::F1)));
    }
}
