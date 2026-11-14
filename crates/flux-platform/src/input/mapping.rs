//! Mapping-слой: действия, значения, биндинги, контексты, поглощение.
//!
//! Действия объявляет геймплей; платформа не знает про «прыжок» (ADR-0031).
//! Разрешение действий детерминировано: результат — `BTreeMap` по UUID
//! действия, независимо от порядка регистрации.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::raw::{MouseButton, RawEvent, RawEventKind, Scancode};

/// Значение действия.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ActionValue {
    /// Кнопка.
    Bool(bool),
    /// Ось.
    Axis1(f64),
    /// Двумерная ось.
    Axis2([f64; 2]),
    /// Трёхмерная ось.
    Axis3([f64; 3]),
}

impl ActionValue {
    /// Активно ли действие.
    #[must_use]
    pub fn is_active(self) -> bool {
        match self {
            Self::Bool(value) => value,
            Self::Axis1(value) => value.abs() > 1e-6,
            Self::Axis2(value) => value.iter().any(|axis| axis.abs() > 1e-6),
            Self::Axis3(value) => value.iter().any(|axis| axis.abs() > 1e-6),
        }
    }

    /// Значение как `bool`.
    #[must_use]
    pub fn as_bool(self) -> bool {
        match self {
            Self::Bool(value) => value,
            _ => self.is_active(),
        }
    }

    /// Первая ось.
    #[must_use]
    pub fn as_axis1(self) -> f64 {
        match self {
            Self::Bool(value) => f64::from(u8::from(value)),
            Self::Axis1(value) => value,
            Self::Axis2(value) => value[0],
            Self::Axis3(value) => value[0],
        }
    }

    /// Первые две оси.
    #[must_use]
    pub fn as_axis2(self) -> [f64; 2] {
        match self {
            Self::Bool(value) => [f64::from(u8::from(value)), 0.0],
            Self::Axis1(value) => [value, 0.0],
            Self::Axis2(value) => value,
            Self::Axis3(value) => [value[0], value[1]],
        }
    }

    fn zero(kind: ActionKind) -> Self {
        match kind {
            ActionKind::Button => Self::Bool(false),
            ActionKind::Axis1 => Self::Axis1(0.0),
            ActionKind::Axis2 => Self::Axis2([0.0; 2]),
            ActionKind::Axis3 => Self::Axis3([0.0; 3]),
        }
    }

    fn accumulate(&mut self, pressed: bool, component: AxisComponent) {
        if !pressed {
            return;
        }
        match self {
            Self::Bool(value) => *value = true,
            Self::Axis1(value) => *value = (*value + component.sign).clamp(-1.0, 1.0),
            Self::Axis2(value) => {
                let axis = usize::from(component.axis.min(1));
                value[axis] = (value[axis] + component.sign).clamp(-1.0, 1.0);
            }
            Self::Axis3(value) => {
                let axis = usize::from(component.axis.min(2));
                value[axis] = (value[axis] + component.sign).clamp(-1.0, 1.0);
            }
        }
    }
}

/// Тип действия.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionKind {
    /// Кнопка.
    Button,
    /// Ось.
    Axis1,
    /// Двумерная ось.
    Axis2,
    /// Трёхмерная ось.
    Axis3,
}

/// Действие: имя для отладки и стабильный UUID для данных.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InputAction {
    /// Имя в коде/конфиге.
    pub name: &'static str,
    /// Стабильный идентификатор (не зависит от имени).
    pub id: Uuid,
    /// Тип значения.
    pub kind: ActionKind,
}

impl InputAction {
    /// Действие с фиксированным UUID.
    #[must_use]
    pub const fn new(name: &'static str, id: Uuid, kind: ActionKind) -> Self {
        Self { name, id, kind }
    }
}

/// Игрок (local multiplayer с первого дня).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct PlayerId(pub u8);

/// Источник биндинга.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BindingSource {
    /// Физическая клавиша.
    Key(Scancode),
    /// Кнопка мыши.
    Mouse(MouseButton),
    /// Кнопка геймпада.
    GamepadButton(u16),
    /// Ось геймпада с направлением (`-1`/`+1`).
    GamepadAxis {
        /// Номер оси.
        axis: u16,
        /// Направление.
        sign: i8,
    },
}

impl BindingSource {
    /// Клавиша.
    #[must_use]
    pub const fn key(scancode: Scancode) -> Self {
        Self::Key(scancode)
    }
}

/// Вклад биндинга в значение действия.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisComponent {
    /// Номер оси (для Axis2/Axis3).
    pub axis: u8,
    /// Знак вклада.
    pub sign: f64,
}

impl Default for AxisComponent {
    fn default() -> Self {
        Self { axis: 0, sign: 1.0 }
    }
}

impl AxisComponent {
    /// Кнопочный вклад (`+1` по первой оси).
    #[must_use]
    pub const fn button() -> Self {
        Self { axis: 0, sign: 1.0 }
    }

    /// Вклад в конкретную ось.
    #[must_use]
    pub const fn axis(axis: u8, sign: f64) -> Self {
        Self { axis, sign }
    }
}

/// Биндинг действия.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    /// Действие.
    pub action: InputAction,
    /// Источник.
    pub source: BindingSource,
    /// Игрок.
    pub player: PlayerId,
    /// Контекст, в котором действует биндинг (`""` — глобальный).
    pub context: &'static str,
    /// Вклад в значение.
    pub component: AxisComponent,
}

impl Binding {
    /// Глобальный биндинг кнопки.
    #[must_use]
    pub fn new(action: InputAction, source: BindingSource, player: PlayerId) -> Self {
        Self {
            action,
            source,
            player,
            context: "",
            component: AxisComponent::button(),
        }
    }

    /// Биндинг в контексте.
    #[must_use]
    pub fn in_context(mut self, context: &'static str) -> Self {
        self.context = context;
        self
    }

    /// Вклад в ось.
    #[must_use]
    pub fn with_component(mut self, component: AxisComponent) -> Self {
        self.component = component;
        self
    }
}

/// Контекст ввода (gameplay/UI/dialogue) с приоритетом поглощения.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InputContext {
    /// Имя контекста.
    pub name: &'static str,
    /// Приоритет: чем выше, тем раньше поглощает.
    pub priority: u8,
}

impl InputContext {
    /// Контекст.
    #[must_use]
    pub const fn new(name: &'static str, priority: u8) -> Self {
        Self { name, priority }
    }

    /// Геймплей.
    pub const GAMEPLAY: Self = Self::new("gameplay", 0);
    /// UI.
    pub const UI: Self = Self::new("ui", 100);
    /// Диалог.
    pub const DIALOGUE: Self = Self::new("dialogue", 50);
}

/// Стек контекстов: верхний поглощает действия нижних.
#[derive(Clone, Debug, Default)]
pub struct ContextStack {
    stack: Vec<InputContext>,
}

impl ContextStack {
    /// Пустой стек.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Кладёт контекст наверх.
    pub fn push(&mut self, context: InputContext) {
        self.stack.push(context);
    }

    /// Снимает верхний контекст.
    pub fn pop(&mut self) -> Option<InputContext> {
        self.stack.pop()
    }

    /// Верхний контекст.
    #[must_use]
    pub fn top(&self) -> Option<InputContext> {
        self.stack.last().copied()
    }

    /// Активные контексты от верхнего к нижнему.
    pub fn active(&self) -> impl Iterator<Item = &InputContext> {
        self.stack.iter().rev()
    }

    /// Есть ли контекст в стеке.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.stack.iter().any(|context| context.name == name)
    }

    /// Число контекстов.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Пуст ли стек.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

/// Состояние устройств после обработки raw-событий.
#[derive(Clone, Debug, Default)]
pub struct InputState {
    keys: BTreeSet<Scancode>,
    buttons: BTreeSet<MouseButton>,
    gamepad_buttons: BTreeSet<u16>,
    gamepad_axes: BTreeMap<u16, f64>,
    mouse_delta: [f64; 2],
    wheel: [f64; 2],
    focused: bool,
    text: String,
}

impl InputState {
    /// Начальное состояние: фокус есть.
    #[must_use]
    pub fn new() -> Self {
        Self {
            focused: true,
            ..Self::default()
        }
    }

    /// Применяет raw-событие.
    pub fn apply(&mut self, event: &RawEvent) {
        match &event.kind {
            RawEventKind::Key {
                scancode, pressed, ..
            } => {
                if *pressed {
                    self.keys.insert(*scancode);
                } else {
                    self.keys.remove(scancode);
                }
            }
            RawEventKind::MouseButton { button, pressed } => {
                if *pressed {
                    self.buttons.insert(*button);
                } else {
                    self.buttons.remove(button);
                }
            }
            RawEventKind::MouseMotion { dx, dy } => {
                self.mouse_delta[0] += dx;
                self.mouse_delta[1] += dy;
            }
            RawEventKind::Wheel { dx, dy } => {
                self.wheel[0] += dx;
                self.wheel[1] += dy;
            }
            RawEventKind::Focus { focused } => {
                self.focused = *focused;
                if !focused {
                    self.keys.clear();
                    self.buttons.clear();
                    self.gamepad_buttons.clear();
                }
            }
            RawEventKind::Text(text) => self.text.push_str(text),
        }
    }

    /// Нажата ли клавиша.
    #[must_use]
    pub fn key_pressed(&self, scancode: Scancode) -> bool {
        self.keys.contains(&scancode)
    }

    /// Активен ли источник.
    #[must_use]
    pub fn source_active(&self, source: BindingSource) -> bool {
        match source {
            BindingSource::Key(scancode) => self.key_pressed(scancode),
            BindingSource::Mouse(button) => self.buttons.contains(&button),
            BindingSource::GamepadButton(button) => self.gamepad_buttons.contains(&button),
            BindingSource::GamepadAxis { axis, sign } => {
                self.gamepad_axes.get(&axis).is_some_and(|value| {
                    if sign >= 0 {
                        *value > 0.0
                    } else {
                        *value < 0.0
                    }
                })
            }
        }
    }

    /// Сырая дельта мыши за кадр.
    #[must_use]
    pub fn mouse_delta(&self) -> [f64; 2] {
        self.mouse_delta
    }

    /// Колесо за кадр.
    #[must_use]
    pub fn wheel(&self) -> [f64; 2] {
        self.wheel
    }

    /// Есть ли фокус.
    #[must_use]
    pub fn focused(&self) -> bool {
        self.focused
    }

    /// Текстовый поток кадра.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Сбрасывает per-frame величины (дельта мыши, колесо, текст).
    pub fn end_frame(&mut self) {
        self.mouse_delta = [0.0; 2];
        self.wheel = [0.0; 2];
        self.text.clear();
    }

    /// Устанавливает значение оси геймпада (адаптер устройств).
    pub fn set_gamepad_axis(&mut self, axis: u16, value: f64) {
        self.gamepad_axes.insert(axis, value.clamp(-1.0, 1.0));
    }

    /// Устанавливает кнопку геймпада.
    pub fn set_gamepad_button(&mut self, button: u16, pressed: bool) {
        if pressed {
            self.gamepad_buttons.insert(button);
        } else {
            self.gamepad_buttons.remove(&button);
        }
    }
}

/// Чорда из двух физических клавиш (порядок нормализуется).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    keys: [Scancode; 2],
}

impl Chord {
    /// Чорда; клавиши сортируются, чтобы `Ctrl+S` и `S+Ctrl` совпадали.
    #[must_use]
    pub fn new(a: Scancode, b: Scancode) -> Self {
        let keys = if a <= b { [a, b] } else { [b, a] };
        Self { keys }
    }

    /// Клавиши в каноническом порядке.
    #[must_use]
    pub fn keys(self) -> [Scancode; 2] {
        self.keys
    }

    /// Активна ли чорда (нажаты обе клавиши).
    #[must_use]
    pub fn is_active(self, state: &InputState) -> bool {
        state.key_pressed(self.keys[0]) && state.key_pressed(self.keys[1])
    }
}

/// Биндинг чорды.
#[derive(Clone, Debug, PartialEq)]
pub struct ChordBinding {
    /// Действие.
    pub action: InputAction,
    /// Чорда.
    pub chord: Chord,
    /// Игрок.
    pub player: PlayerId,
    /// Контекст (`""` — глобальный).
    pub context: &'static str,
    /// Вклад в значение.
    pub component: AxisComponent,
}

impl ChordBinding {
    /// Глобальная чорда.
    #[must_use]
    pub fn new(action: InputAction, chord: Chord, player: PlayerId) -> Self {
        Self {
            action,
            chord,
            player,
            context: "",
            component: AxisComponent::button(),
        }
    }

    /// Чорда в контексте.
    #[must_use]
    pub fn in_context(mut self, context: &'static str) -> Self {
        self.context = context;
        self
    }

    /// Вклад в ось.
    #[must_use]
    pub fn with_component(mut self, component: AxisComponent) -> Self {
        self.component = component;
        self
    }
}

/// Карта действий: биндинги, чорды и стек контекстов.
#[derive(Clone, Debug, Default)]
pub struct InputMap {
    bindings: Vec<Binding>,
    chords: Vec<ChordBinding>,
    contexts: ContextStack,
}

impl InputMap {
    /// Пустая карта.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавляет биндинг.
    pub fn bind(&mut self, binding: Binding) {
        self.bindings.push(binding);
    }

    /// Добавляет биндинг чорды.
    pub fn bind_chord(&mut self, binding: ChordBinding) {
        self.chords.push(binding);
    }

    /// Биндинги (только чтение).
    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// Биндинги чорд (только чтение).
    #[must_use]
    pub fn chords(&self) -> &[ChordBinding] {
        &self.chords
    }

    /// Стек контекстов.
    #[must_use]
    pub fn contexts(&self) -> &ContextStack {
        &self.contexts
    }

    /// Мутабельный стек контекстов.
    pub fn contexts_mut(&mut self) -> &mut ContextStack {
        &mut self.contexts
    }

    /// Разрешает действия игрока.
    ///
    /// Порядок обхода: активные контексты от верхнего к нижнему, затем
    /// глобальные биндинги. Внутри контекста все биндинги одного действия
    /// накапливаются. Модальный контекст (`priority > gameplay`) поглощает
    /// и действие, и физические источники, которые он использует: нижние
    /// слои их не видят. Геймплейный контекст источники не захватывает —
    /// глобальные биндинги остаются резервным слоем.
    ///
    /// Результат отсортирован по UUID действия.
    #[must_use]
    pub fn resolve(&self, state: &InputState, player: PlayerId) -> BTreeMap<Uuid, ActionValue> {
        let mut out = ResolveState::default();

        for context in self.contexts.active() {
            let claim_sources = context.priority > InputContext::GAMEPLAY.priority;
            self.resolve_context(state, player, context.name, claim_sources, &mut out);
        }
        self.resolve_context(state, player, "", false, &mut out);
        out.values
    }

    fn resolve_context(
        &self,
        state: &InputState,
        player: PlayerId,
        context: &str,
        claim_sources: bool,
        out: &mut ResolveState,
    ) {
        let mut context_sources: BTreeSet<BindingSource> = BTreeSet::new();
        let mut touched_actions: BTreeSet<Uuid> = BTreeSet::new();

        for binding in self
            .bindings
            .iter()
            .filter(|binding| binding.player == player && binding.context == context)
        {
            if out.absorbed_actions.contains(&binding.action.id) {
                continue;
            }
            if out.absorbed_sources.contains(&binding.source) {
                continue;
            }
            let entry = out
                .values
                .entry(binding.action.id)
                .or_insert_with(|| ActionValue::zero(binding.action.kind));
            entry.accumulate(state.source_active(binding.source), binding.component);
            touched_actions.insert(binding.action.id);
            if claim_sources {
                context_sources.insert(binding.source);
            }
        }

        for binding in self
            .chords
            .iter()
            .filter(|binding| binding.player == player && binding.context == context)
        {
            if out.absorbed_actions.contains(&binding.action.id) {
                continue;
            }
            let keys_absorbed = binding
                .chord
                .keys()
                .iter()
                .any(|key| out.absorbed_sources.contains(&BindingSource::Key(*key)));
            if keys_absorbed {
                continue;
            }
            let active = binding.chord.is_active(state);
            let entry = out
                .values
                .entry(binding.action.id)
                .or_insert_with(|| ActionValue::zero(binding.action.kind));
            entry.accumulate(active, binding.component);
            touched_actions.insert(binding.action.id);
            // Модальная чорда захватывает клавиши только когда она активна:
            // иначе `Ctrl+S` в UI блокировал бы `S` в геймплее постоянно.
            if claim_sources && active {
                for key in binding.chord.keys() {
                    context_sources.insert(BindingSource::Key(key));
                }
            }
        }

        out.absorbed_actions.extend(touched_actions);
        out.absorbed_sources.extend(context_sources);
    }
}

/// Аккумулятор разрешения действий (значения + поглощённые действия/источники).
#[derive(Default)]
struct ResolveState {
    values: BTreeMap<Uuid, ActionValue>,
    absorbed_actions: BTreeSet<Uuid>,
    absorbed_sources: BTreeSet<BindingSource>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const MOVE: InputAction = InputAction::new(
        "move",
        Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0300),
        ActionKind::Axis2,
    );
    const JUMP: InputAction = InputAction::new(
        "jump",
        Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0301),
        ActionKind::Button,
    );
    const CONFIRM: InputAction = InputAction::new(
        "confirm",
        Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0302),
        ActionKind::Button,
    );

    fn press(state: &mut InputState, scancode: Scancode) {
        state.apply(&RawEvent::new(
            Duration::ZERO,
            RawEventKind::Key {
                scancode,
                pressed: true,
                repeat: false,
            },
        ));
    }

    fn wasd_map() -> InputMap {
        let mut map = InputMap::new();
        for (scancode, axis, sign) in [
            (Scancode::W, 1, 1.0),
            (Scancode::S, 1, -1.0),
            (Scancode::D, 0, 1.0),
            (Scancode::A, 0, -1.0),
        ] {
            map.bind(
                Binding::new(MOVE, BindingSource::key(scancode), PlayerId(0))
                    .with_component(AxisComponent::axis(axis, sign)),
            );
        }
        map.bind(Binding::new(
            JUMP,
            BindingSource::key(Scancode::SPACE),
            PlayerId(0),
        ));
        map.bind(Binding::new(
            CONFIRM,
            BindingSource::key(Scancode::SPACE),
            PlayerId(0),
        ));
        map
    }

    #[test]
    fn wasd_resolves_to_axis2_by_scancode() {
        let mut state = InputState::new();
        let mut map = wasd_map();
        map.contexts_mut().push(InputContext::GAMEPLAY);

        press(&mut state, Scancode::W);
        press(&mut state, Scancode::D);
        let values = map.resolve(&state, PlayerId(0));
        let movement = values
            .get(&MOVE.id)
            .copied()
            .unwrap_or(ActionValue::Axis2([0.0, 0.0]));
        assert_eq!(movement.as_axis2(), [1.0, 1.0]);

        press(&mut state, Scancode::SPACE);
        let values = map.resolve(&state, PlayerId(0));
        assert!(values.get(&JUMP.id).is_some_and(|value| value.as_bool()));
    }

    #[test]
    fn ui_context_absorbs_gameplay_action() {
        let mut state = InputState::new();
        let mut map = wasd_map();
        map.contexts_mut().push(InputContext::GAMEPLAY);
        map.bind(
            Binding::new(CONFIRM, BindingSource::key(Scancode::SPACE), PlayerId(0))
                .in_context("ui"),
        );

        press(&mut state, Scancode::SPACE);
        // Без UI-контекста confirm берётся из глобального биндинга.
        assert!(map.resolve(&state, PlayerId(0)).contains_key(&CONFIRM.id));

        map.contexts_mut().push(InputContext::UI);
        let values = map.resolve(&state, PlayerId(0));
        assert!(values.get(&CONFIRM.id).is_some_and(|value| value.as_bool()));
        // Jump тоже поглощён: UI-биндинг confirm перекрыл глобальный, а
        // отдельного jump в UI нет — он не виден.
        assert!(!values.contains_key(&JUMP.id));
    }

    #[test]
    fn result_order_is_deterministic_by_uuid() {
        let mut state = InputState::new();
        let mut map = wasd_map();
        map.contexts_mut().push(InputContext::GAMEPLAY);
        press(&mut state, Scancode::SPACE);

        let first: Vec<Uuid> = map.resolve(&state, PlayerId(0)).keys().copied().collect();
        let second: Vec<Uuid> = map.resolve(&state, PlayerId(0)).keys().copied().collect();
        assert_eq!(first, second);
        let mut sorted = first.clone();
        sorted.sort_unstable();
        assert_eq!(first, sorted);
    }

    #[test]
    fn focus_loss_releases_keys() {
        let mut state = InputState::new();
        press(&mut state, Scancode::W);
        assert!(state.key_pressed(Scancode::W));
        state.apply(&RawEvent::new(
            Duration::ZERO,
            RawEventKind::Focus { focused: false },
        ));
        assert!(!state.key_pressed(Scancode::W));
        assert!(!state.focused());
    }

    #[test]
    fn players_are_isolated() {
        let mut state = InputState::new();
        let mut map = wasd_map();
        map.contexts_mut().push(InputContext::GAMEPLAY);
        press(&mut state, Scancode::W);

        assert!(map.resolve(&state, PlayerId(0)).contains_key(&MOVE.id));
        assert!(map.resolve(&state, PlayerId(1)).is_empty());
    }

    #[test]
    fn chord_requires_both_keys_and_is_order_independent() {
        let mut state = InputState::new();
        let mut map = InputMap::new();
        map.bind_chord(ChordBinding::new(
            CONFIRM,
            Chord::new(Scancode::LEFT_CTRL, Scancode::S),
            PlayerId(0),
        ));
        map.contexts_mut().push(InputContext::GAMEPLAY);

        press(&mut state, Scancode::LEFT_CTRL);
        let values = map.resolve(&state, PlayerId(0));
        assert!(!values.get(&CONFIRM.id).is_some_and(|value| value.as_bool()));

        press(&mut state, Scancode::S);
        let values = map.resolve(&state, PlayerId(0));
        assert!(values.get(&CONFIRM.id).is_some_and(|value| value.as_bool()));

        assert_eq!(
            Chord::new(Scancode::S, Scancode::LEFT_CTRL),
            Chord::new(Scancode::LEFT_CTRL, Scancode::S)
        );
    }

    #[test]
    fn active_modal_chord_absorbs_its_keys() {
        let mut state = InputState::new();
        let mut map = wasd_map();
        map.contexts_mut().push(InputContext::GAMEPLAY);
        map.bind_chord(
            ChordBinding::new(
                CONFIRM,
                Chord::new(Scancode::LEFT_CTRL, Scancode::S),
                PlayerId(0),
            )
            .in_context("ui"),
        );
        map.bind(
            Binding::new(JUMP, BindingSource::key(Scancode::S), PlayerId(0)).in_context("gameplay"),
        );

        press(&mut state, Scancode::LEFT_CTRL);
        press(&mut state, Scancode::S);
        map.contexts_mut().push(InputContext::UI);
        let values = map.resolve(&state, PlayerId(0));

        assert!(values.get(&CONFIRM.id).is_some_and(|value| value.as_bool()));
        // S захвачен активной UI-чордой, поэтому gameplay-прыжок не активен.
        assert!(!values.get(&JUMP.id).is_some_and(|value| value.as_bool()));
    }
}
