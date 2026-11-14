//! KCC-минимум: чистая функция `move_character` поверх collision-минимума.
//!
//! Всё состояние — в [`CharacterState`]; скрытых полей контроллера нет
//! (ADR-0024). Порядок тика: Input → Controller → Physics → ControllerPost.
//! Функция не знает про ECS и не аллоцирует: системы кладут состояние в
//! компоненты и вызывают её на тике.

use serde::{Deserialize, Serialize};

use flux_core::flux_math::DVec3;
use flux_core::input::Intention;

use crate::collision::{Capsule, TileGrid};

/// Максимум итераций скольжения по поверхности.
const MAX_SLIDES: usize = 4;

/// Состояние персонажа (компонент ECS).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterState {
    /// Позиция центра капсулы.
    pub position: DVec3,
    /// Скорость.
    pub velocity: DVec3,
    /// Стоит ли на поверхности (с учётом гистерезиса).
    pub grounded: bool,
    /// Нормаль последней опоры.
    pub ground_normal: DVec3,
    /// Оставшиеся тики гистерезиса.
    pub ground_hysteresis: u8,
}

impl Default for CharacterState {
    fn default() -> Self {
        Self {
            position: DVec3::ZERO,
            velocity: DVec3::ZERO,
            grounded: false,
            ground_normal: DVec3::Y,
            ground_hysteresis: 0,
        }
    }
}

/// Параметры персонажа (компонент ECS/конфиг).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterConfig {
    /// Радиус капсулы.
    pub radius: f64,
    /// Половина высоты цилиндрической части.
    pub half_height: f64,
    /// Максимальная горизонтальная скорость.
    pub max_speed: f64,
    /// Ускорение на земле.
    pub acceleration: f64,
    /// Ускорение в воздухе.
    pub air_acceleration: f64,
    /// Гравитация.
    pub gravity: f64,
    /// Скорость прыжка.
    pub jump_speed: f64,
    /// Предельная скорость падения.
    pub max_fall_speed: f64,
    /// Высота ступени (задел для I2: step-up).
    pub step_height: f64,
    /// Косинус предельного угла склона.
    pub slope_limit_cos: f64,
    /// Тиков гистерезиса опоры.
    pub ground_hysteresis_ticks: u8,
    /// Глубина пробы пола под капсулой.
    pub ground_probe: f64,
    /// Множитель скорости в приседе.
    pub crouch_speed_scale: f64,
}

impl Default for CharacterConfig {
    fn default() -> Self {
        Self {
            radius: 0.3,
            half_height: 0.9,
            max_speed: 6.0,
            acceleration: 60.0,
            air_acceleration: 12.0,
            gravity: 20.0,
            jump_speed: 6.0,
            max_fall_speed: 50.0,
            step_height: 0.3,
            slope_limit_cos: std::f64::consts::FRAC_1_SQRT_2,
            ground_hysteresis_ticks: 3,
            ground_probe: 0.05,
            crouch_speed_scale: 0.5,
        }
    }
}

/// Результат шага KCC.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveResult {
    /// Новое состояние.
    pub state: CharacterState,
    /// Был ли контакт со стеной/полом/потолком.
    pub hit: bool,
}

/// Один шаг KCC: чистая функция `(state, config, intention, dt, grid)`.
///
/// Имя `move` занято ключевым словом Rust, поэтому функция называется
/// `move_character`; контракт из ADR-0024 сохраняется.
#[must_use]
pub fn move_character(
    state: &CharacterState,
    config: &CharacterConfig,
    intention: &Intention,
    dt: f64,
    grid: &TileGrid,
) -> MoveResult {
    if !dt.is_finite() || dt <= 0.0 {
        return MoveResult {
            state: *state,
            hit: false,
        };
    }

    let intention = intention.clamped();
    let was_grounded = state.grounded || state.ground_hysteresis > 0;
    let speed_scale = if intention.crouch {
        config.crouch_speed_scale
    } else {
        1.0
    };

    let mut velocity = state.velocity;
    let target_x = intention.move_axis[0] * config.max_speed * speed_scale;
    let acceleration = if was_grounded {
        config.acceleration
    } else {
        config.air_acceleration
    };
    velocity.x = approach(velocity.x, target_x, acceleration * dt);

    let jumped = intention.jump && was_grounded;
    if jumped {
        velocity.y = config.jump_speed;
    }
    velocity.y = (velocity.y - config.gravity * dt).max(-config.max_fall_speed);

    let capsule = Capsule::new(config.radius, config.half_height);
    let mut position = state.position;
    let mut remaining = velocity * dt;
    let mut hit = false;
    let mut ground_contact = false;
    let mut ground_normal = DVec3::Y;
    let mut blocked_wall = false;

    for _ in 0..MAX_SLIDES {
        if remaining.length_squared() <= 1.0e-24 {
            break;
        }
        let target = position + remaining;
        match grid.sweep_capsule(&capsule, position, target) {
            None => {
                position = target;
                remaining = DVec3::ZERO;
            }
            Some(contact) => {
                hit = true;
                position += remaining * contact.time;
                let normal = contact.normal;
                if normal.y >= config.slope_limit_cos {
                    ground_contact = true;
                    ground_normal = normal;
                    if velocity.y < 0.0 {
                        velocity.y = 0.0;
                    }
                } else if normal.y <= -config.slope_limit_cos && velocity.y > 0.0 {
                    velocity.y = 0.0;
                } else if normal.x.abs() > 0.5 {
                    blocked_wall = true;
                }
                let into = remaining.dot(normal);
                remaining -= normal * into;
            }
        }
    }

    // Step-up: если на земле упёрлись в вертикальную грань, пробуем подняться
    // на `step_height` и проехать горизонтально. Провал попытки — остаёмся
    // на месте (никакого «телепорта» вверх).
    if was_grounded && blocked_wall {
        if let Some(stepped) = try_step_up(state.position, velocity.x * dt, config, &capsule, grid)
        {
            if stepped.y >= position.y - 1.0e-3 {
                position = stepped;
                velocity.y = 0.0;
                ground_contact = true;
                ground_normal = DVec3::Y;
            }
        }
    }

    if !ground_contact && !jumped {
        let probe = config.ground_probe.max(0.0);
        if probe > 0.0 {
            if let Some(contact) =
                grid.sweep_capsule(&capsule, position, position - DVec3::Y * probe)
            {
                if contact.normal.y >= config.slope_limit_cos {
                    ground_contact = true;
                    ground_normal = contact.normal;
                }
            }
        }
    }

    let hysteresis = if jumped {
        0
    } else if ground_contact {
        config.ground_hysteresis_ticks
    } else {
        state.ground_hysteresis.saturating_sub(1)
    };
    let grounded = hysteresis > 0;
    let ground_normal = if ground_contact {
        ground_normal
    } else if grounded {
        state.ground_normal
    } else {
        DVec3::Y
    };

    MoveResult {
        state: CharacterState {
            position,
            velocity,
            grounded,
            ground_normal,
            ground_hysteresis: hysteresis,
        },
        hit,
    }
}

fn approach(current: f64, target: f64, max_delta: f64) -> f64 {
    let delta = target - current;
    if delta.abs() <= max_delta {
        target
    } else {
        current + delta.signum() * max_delta
    }
}

/// Попытка шага вверх: подъём на `step_height`, горизонтальный проезд,
/// спуск обратно. Возвращает новую позицию только при полном успехе.
fn try_step_up(
    position: DVec3,
    horizontal: f64,
    config: &CharacterConfig,
    capsule: &Capsule,
    grid: &TileGrid,
) -> Option<DVec3> {
    if horizontal.abs() <= 1.0e-6 || !config.step_height.is_finite() || config.step_height <= 0.0 {
        return None;
    }

    let lift = DVec3::new(0.0, config.step_height, 0.0);
    if grid
        .sweep_capsule(capsule, position, position + lift)
        .is_some()
    {
        return None;
    }
    let lifted = position + lift;
    let target = lifted + DVec3::new(horizontal, 0.0, 0.0);
    if grid.sweep_capsule(capsule, lifted, target).is_some() {
        return None;
    }

    let down = DVec3::new(
        0.0,
        -(config.step_height + config.ground_probe.max(0.0)),
        0.0,
    );
    let landed = match grid.sweep_capsule(capsule, target, target + down) {
        Some(contact) => target + down * contact.time,
        None => target + down,
    };
    (landed.y >= position.y - 1.0e-3).then_some(landed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 60.0;

    fn scene() -> TileGrid {
        // 10×4: пол внизу, стена на x=7, y=1.
        TileGrid::from_rows(&["..........", "..........", ".......#..", "##########"])
    }

    fn standing_state(config: &CharacterConfig) -> CharacterState {
        CharacterState {
            position: DVec3::new(2.0, 1.0 + config.half_height + config.radius, 0.0),
            ..CharacterState::default()
        }
    }

    #[test]
    fn character_stands_on_floor() {
        let grid = scene();
        let config = CharacterConfig::default();
        let mut state = standing_state(&config);
        for _ in 0..30 {
            state = move_character(&state, &config, &Intention::IDLE, DT, &grid).state;
        }
        assert!(state.grounded);
        assert!(state.ground_hysteresis > 0);
        let expected_y = 1.0 + config.half_height + config.radius;
        assert!((state.position.y - expected_y).abs() < 1e-3);
        assert!(state.velocity.y.abs() < 1e-9);
    }

    #[test]
    fn wall_blocks_horizontal_motion() {
        let grid = scene();
        let config = CharacterConfig::default();
        let mut state = standing_state(&config);
        let intention = Intention {
            move_axis: [1.0, 0.0],
            ..Intention::IDLE
        };

        for _ in 0..180 {
            state = move_character(&state, &config, &intention, DT, &grid).state;
        }

        // Стена x=7: расширенная граница 7.0 - radius.
        let limit = 7.0 - config.radius;
        assert!(state.position.x <= limit + 1e-3);
        assert!(state.position.x > 5.0);
    }

    #[test]
    fn jump_leaves_ground_and_lands() {
        let grid = scene();
        let config = CharacterConfig::default();
        let mut state = standing_state(&config);

        // Сначала встаём на пол.
        for _ in 0..10 {
            state = move_character(&state, &config, &Intention::IDLE, DT, &grid).state;
        }
        let start_y = state.position.y;

        let jump = Intention {
            jump: true,
            ..Intention::IDLE
        };
        state = move_character(&state, &config, &jump, DT, &grid).state;
        assert!(!state.grounded);
        assert!(state.velocity.y > 0.0);

        let mut max_y = state.position.y;
        for _ in 0..20 {
            state = move_character(&state, &config, &Intention::IDLE, DT, &grid).state;
            max_y = max_y.max(state.position.y);
        }
        assert!(max_y > start_y + 0.2);

        for _ in 0..240 {
            state = move_character(&state, &config, &Intention::IDLE, DT, &grid).state;
        }
        assert!(state.grounded);
        assert!((state.position.y - start_y).abs() < 1e-3);
    }

    #[test]
    fn two_runs_are_identical() {
        let grid = scene();
        let config = CharacterConfig::default();
        let intention = Intention {
            move_axis: [1.0, 0.0],
            ..Intention::IDLE
        };

        let run = || {
            let mut state = standing_state(&config);
            for _ in 0..200 {
                state = move_character(&state, &config, &intention, DT, &grid).state;
            }
            state
        };

        assert_eq!(run(), run());
    }

    #[test]
    fn crouch_reduces_speed() {
        let grid = scene();
        let config = CharacterConfig::default();
        let mut walk = standing_state(&config);
        let mut crouch = standing_state(&config);
        let walking = Intention {
            move_axis: [1.0, 0.0],
            ..Intention::IDLE
        };
        let crouching = Intention {
            move_axis: [1.0, 0.0],
            crouch: true,
            ..Intention::IDLE
        };

        for _ in 0..30 {
            walk = move_character(&walk, &config, &walking, DT, &grid).state;
            crouch = move_character(&crouch, &config, &crouching, DT, &grid).state;
        }
        assert!(crouch.velocity.x < walk.velocity.x);
    }

    #[test]
    fn step_up_climbs_one_tile() {
        let grid = TileGrid::from_rows(&["..........", "..........", "....######", "##########"]);
        let config = CharacterConfig {
            step_height: 1.0,
            ..CharacterConfig::default()
        };
        let mut state = standing_state(&config);
        let intention = Intention {
            move_axis: [1.0, 0.0],
            ..Intention::IDLE
        };

        for _ in 0..60 {
            state = move_character(&state, &config, &intention, DT, &grid).state;
        }

        // Ступень высотой 1.0: стоим на её верхней грани (y = 2.0 + 1.2).
        assert!(state.position.y > 3.0, "y = {}", state.position.y);
        assert!(state.position.x > 4.5, "x = {}", state.position.x);
        assert!(state.grounded);
    }

    #[test]
    fn bot_trace_is_deterministic() {
        let grid = scene();
        let config = CharacterConfig::default();

        let run = || {
            let mut state = standing_state(&config);
            let right = Intention {
                move_axis: [1.0, 0.0],
                ..Intention::IDLE
            };
            let jump = Intention {
                jump: true,
                ..Intention::IDLE
            };
            for _ in 0..30 {
                state = move_character(&state, &config, &right, DT, &grid).state;
            }
            state = move_character(&state, &config, &jump, DT, &grid).state;
            for _ in 0..30 {
                state = move_character(&state, &config, &right, DT, &grid).state;
            }
            for _ in 0..120 {
                state = move_character(&state, &config, &Intention::IDLE, DT, &grid).state;
            }
            state
        };

        let first = run();
        let second = run();
        assert_eq!(first, second);
        assert!(first.grounded);
    }

    #[test]
    fn character_state_roundtrips_through_ron() {
        let state = CharacterState {
            position: DVec3::new(1.5, 2.25, -0.5),
            velocity: DVec3::new(3.0, -1.0, 0.0),
            grounded: true,
            ground_normal: DVec3::Y,
            ground_hysteresis: 2,
        };
        let text = ron::ser::to_string(&state).unwrap();
        let restored: CharacterState = ron::from_str(&text).unwrap();
        assert_eq!(state, restored);
    }
}
