//! Collision-минимум: тайловый grid, капсула, луч, детерминированные свипы.
//!
//! Без solver: запросы возвращают [`Contact`], решение о движении принимает
//! KCC (ADR-0032). Тайлы перебираются в порядке `(y, x)`; при равном времени
//! контакта побеждает меньший индекс. Никаких `HashMap`-итераций.

use serde::{Deserialize, Serialize};

use flux_core::flux_math::DVec3;

const EPSILON: f64 = 1.0e-12;
/// Зазор свипа: капсула останавливается чуть раньше поверхности, чтобы
/// горизонтальное скольжение по полу не считалось столкновением.
const SKIN: f64 = 1.0e-4;

/// Вертикальная капсула.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Capsule {
    /// Радиус.
    pub radius: f64,
    /// Половина высоты цилиндрической части.
    pub half_height: f64,
}

impl Capsule {
    /// Капсула.
    #[must_use]
    pub const fn new(radius: f64, half_height: f64) -> Self {
        Self {
            radius,
            half_height,
        }
    }

    /// Мировой AABB капсулы в позиции `position`.
    #[must_use]
    pub fn aabb(self, position: DVec3) -> (DVec3, DVec3) {
        let half = DVec3::new(
            self.radius.max(0.0),
            self.half_height.max(0.0) + self.radius.max(0.0),
            self.radius.max(0.0),
        );
        (position - half, position + half)
    }
}

/// Луч.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ray {
    /// Начало.
    pub origin: DVec3,
    /// Направление (нормализуется при запросе).
    pub direction: DVec3,
    /// Максимальная дистанция.
    pub max_distance: f64,
}

impl Ray {
    /// Луч.
    #[must_use]
    pub const fn new(origin: DVec3, direction: DVec3, max_distance: f64) -> Self {
        Self {
            origin,
            direction,
            max_distance,
        }
    }
}

/// Контакт свипа/луча.
///
/// Для [`TileGrid::sweep_capsule`] `time` — доля `0..1` вдоль перемещения;
/// для [`TileGrid::raycast`] — дистанция вдоль нормализованного луча.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    /// Время/дистанция контакта.
    pub time: f64,
    /// Нормаль поверхности (единичная по оси).
    pub normal: DVec3,
    /// Точка контакта.
    pub point: DVec3,
}

/// Ячейка тайлового grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tile {
    /// Пусто.
    Empty,
    /// Твёрдая ячейка.
    Solid,
}

/// Тайловый grid в плоскости XY (капсула вертикальна вдоль Y).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TileGrid {
    width: usize,
    height: usize,
    tile_size: f64,
    tiles: Vec<Tile>,
}

impl TileGrid {
    /// Строит grid из ASCII-строк: `#` — твёрдая ячейка, остальное — пусто.
    ///
    /// Первая строка — верхний ряд (наибольший `y`); `y = 0` — нижний ряд.
    #[must_use]
    pub fn from_rows(rows: &[&str]) -> Self {
        Self::from_rows_with_tile_size(rows, 1.0)
    }

    /// То же с размером ячейки.
    #[must_use]
    pub fn from_rows_with_tile_size(rows: &[&str], tile_size: f64) -> Self {
        let height = rows.len();
        let width = rows
            .iter()
            .map(|row| row.chars().count())
            .max()
            .unwrap_or(0);
        let mut tiles = vec![Tile::Empty; width * height];
        for (row_index, row) in rows.iter().enumerate() {
            let y = height.saturating_sub(1 + row_index);
            for (x, symbol) in row.chars().enumerate() {
                if symbol == '#' {
                    tiles[y * width + x] = Tile::Solid;
                }
            }
        }
        Self {
            width,
            height,
            tile_size: if tile_size.is_finite() && tile_size > 0.0 {
                tile_size
            } else {
                1.0
            },
            tiles,
        }
    }

    /// Ширина в ячейках.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Высота в ячейках.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Размер ячейки.
    #[must_use]
    pub fn tile_size(&self) -> f64 {
        self.tile_size
    }

    /// Пуст ли grid.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Ячейка по индексам.
    #[must_use]
    pub fn tile(&self, x: usize, y: usize) -> Option<Tile> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.tiles.get(y * self.width + x).copied()
    }

    /// Твёрдая ли ячейка.
    #[must_use]
    pub fn is_solid(&self, x: usize, y: usize) -> bool {
        matches!(self.tile(x, y), Some(Tile::Solid))
    }

    /// Ячейка, содержащая точку.
    #[must_use]
    pub fn cell_of(&self, point: DVec3) -> Option<(usize, usize)> {
        let x = (point.x / self.tile_size).floor();
        let y = (point.y / self.tile_size).floor();
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let (x, y) = (x as usize, y as usize);
        (x < self.width && y < self.height).then_some((x, y))
    }

    /// Твёрдая ли точка мира.
    #[must_use]
    pub fn is_solid_at(&self, point: DVec3) -> bool {
        self.cell_of(point)
            .is_some_and(|(x, y)| self.is_solid(x, y))
    }

    /// Границы ячейки в мире.
    #[must_use]
    pub fn cell_bounds(&self, x: usize, y: usize) -> Option<(DVec3, DVec3)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let size = self.tile_size;
        let min = DVec3::new(x as f64 * size, y as f64 * size, 0.0);
        let max = DVec3::new((x + 1) as f64 * size, (y + 1) as f64 * size, 0.0);
        Some((min, max))
    }

    /// Свип вертикальной капсулы из `from` в `to`.
    ///
    /// Возвращает первый контакт; при равном времени — контакт с меньшим
    /// индексом `(y, x)`.
    #[must_use]
    pub fn sweep_capsule(&self, capsule: &Capsule, from: DVec3, to: DVec3) -> Option<Contact> {
        let delta = to - from;
        let displacement = [delta.x, delta.y];
        if displacement[0] * displacement[0] + displacement[1] * displacement[1]
            <= EPSILON * EPSILON
        {
            return None;
        }
        let origin = [from.x, from.y];
        let radius = (capsule.radius - SKIN).max(0.0);
        let vertical = (capsule.half_height + capsule.radius - SKIN).max(0.0);

        let mut best: Option<(f64, [f64; 2])> = None;
        for (index, tile) in self.tiles.iter().enumerate() {
            if *tile != Tile::Solid {
                continue;
            }
            let x = index % self.width;
            let y = index / self.width;
            let size = self.tile_size;
            let min = [x as f64 * size - radius, y as f64 * size - vertical];
            let max = [
                (x + 1) as f64 * size + radius,
                (y + 1) as f64 * size + vertical,
            ];
            if let Some((time, normal)) = ray_aabb_2d(origin, displacement, min, max) {
                if (0.0..=1.0).contains(&time) && is_better(best, time) {
                    best = Some((time, normal));
                }
            }
        }

        best.map(|(time, normal)| Contact {
            time,
            normal: DVec3::new(normal[0], normal[1], 0.0),
            point: DVec3::new(from.x + delta.x * time, from.y + delta.y * time, from.z),
        })
    }

    /// Луч по тайлам.
    ///
    /// `Contact::time` — дистанция вдоль нормализованного направления.
    #[must_use]
    pub fn raycast(&self, ray: &Ray) -> Option<Contact> {
        let length = ray.direction.length();
        if length <= EPSILON || !ray.max_distance.is_finite() || ray.max_distance <= 0.0 {
            return None;
        }
        let direction = [ray.direction.x / length, ray.direction.y / length];
        let origin = [ray.origin.x, ray.origin.y];

        let mut best: Option<(f64, [f64; 2])> = None;
        for (index, tile) in self.tiles.iter().enumerate() {
            if *tile != Tile::Solid {
                continue;
            }
            let x = index % self.width;
            let y = index / self.width;
            let size = self.tile_size;
            let min = [x as f64 * size, y as f64 * size];
            let max = [(x + 1) as f64 * size, (y + 1) as f64 * size];
            if let Some((time, normal)) = ray_aabb_2d(origin, direction, min, max) {
                if (0.0..=ray.max_distance).contains(&time) && is_better(best, time) {
                    best = Some((time, normal));
                }
            }
        }

        best.map(|(time, normal)| Contact {
            time,
            normal: DVec3::new(normal[0], normal[1], 0.0),
            point: DVec3::new(
                ray.origin.x + direction[0] * time,
                ray.origin.y + direction[1] * time,
                ray.origin.z,
            ),
        })
    }
}

fn is_better(best: Option<(f64, [f64; 2])>, time: f64) -> bool {
    match best {
        None => true,
        Some((best_time, _)) => time < best_time - EPSILON,
    }
}

/// Пересечение луча с AABB в плоскости XY.
///
/// Возвращает `(t, normal)`; при старте строго внутри — `t = 0` и нормаль
/// минимального проникновения. Касание границы при движении наружу
/// столкновением не считается (иначе капсула «залипает» на полу).
fn ray_aabb_2d(
    origin: [f64; 2],
    direction: [f64; 2],
    min: [f64; 2],
    max: [f64; 2],
) -> Option<(f64, [f64; 2])> {
    let mut enter = f64::NEG_INFINITY;
    let mut exit = f64::INFINITY;
    let mut normal = [0.0; 2];

    for axis in 0..2 {
        if direction[axis].abs() <= EPSILON {
            if origin[axis] <= min[axis] + EPSILON || origin[axis] >= max[axis] - EPSILON {
                return None;
            }
        } else {
            let inverse = 1.0 / direction[axis];
            let mut near = (min[axis] - origin[axis]) * inverse;
            let mut far = (max[axis] - origin[axis]) * inverse;
            let mut axis_normal = -1.0;
            if near > far {
                std::mem::swap(&mut near, &mut far);
                axis_normal = 1.0;
            }
            if near > enter {
                enter = near;
                normal = [0.0; 2];
                normal[axis] = axis_normal;
            }
            if far < exit {
                exit = far;
            }
            if enter > exit {
                return None;
            }
        }
    }

    if exit < 0.0 {
        return None;
    }
    if enter < 0.0 {
        let inside = origin[0] > min[0] + EPSILON
            && origin[0] < max[0] - EPSILON
            && origin[1] > min[1] + EPSILON
            && origin[1] < max[1] - EPSILON;
        if inside {
            Some((0.0, penetration_normal(origin, min, max)))
        } else {
            None
        }
    } else {
        Some((enter, normal))
    }
}

fn penetration_normal(point: [f64; 2], min: [f64; 2], max: [f64; 2]) -> [f64; 2] {
    let left = point[0] - min[0];
    let right = max[0] - point[0];
    let down = point[1] - min[1];
    let up = max[1] - point[1];
    let smallest = left.min(right).min(down).min(up);
    if smallest == left {
        [-1.0, 0.0]
    } else if smallest == right {
        [1.0, 0.0]
    } else if smallest == down {
        [0.0, -1.0]
    } else {
        [0.0, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> TileGrid {
        // 8×4: пол внизу (y=0), стена на x=5, y=1.
        TileGrid::from_rows(&["........", "........", ".....#..", "########"])
    }

    #[test]
    fn rows_are_parsed_top_to_bottom() {
        let grid = TileGrid::from_rows(&["..", "#."]);
        assert_eq!(grid.width(), 2);
        assert_eq!(grid.height(), 2);
        // Верхняя строка — y=1.
        assert!(!grid.is_solid(0, 1));
        assert!(grid.is_solid(0, 0));
        assert!(!grid.is_solid(1, 0));
    }

    #[test]
    fn cell_of_maps_world_to_index() {
        let grid = scene();
        assert_eq!(grid.cell_of(DVec3::new(0.5, 0.5, 0.0)), Some((0, 0)));
        assert_eq!(grid.cell_of(DVec3::new(5.5, 1.5, 0.0)), Some((5, 1)));
        assert_eq!(grid.cell_of(DVec3::new(-0.1, 0.5, 0.0)), None);
        assert!(grid.is_solid_at(DVec3::new(5.5, 1.5, 0.0)));
    }

    #[test]
    fn raycast_hits_wall_with_axis_normal() {
        let grid = scene();
        let ray = Ray::new(DVec3::new(1.0, 1.5, 0.0), DVec3::new(1.0, 0.0, 0.0), 10.0);
        let contact = grid.raycast(&ray).unwrap();
        // Стена x=5 начинается на 5.0; луч стартует в 1.0.
        assert!((contact.time - 4.0).abs() < 1e-12);
        assert_eq!(contact.normal, DVec3::new(-1.0, 0.0, 0.0));
    }

    #[test]
    fn raycast_respects_max_distance() {
        let grid = scene();
        let ray = Ray::new(DVec3::new(1.0, 1.5, 0.0), DVec3::new(1.0, 0.0, 0.0), 2.0);
        assert!(grid.raycast(&ray).is_none());
    }

    #[test]
    fn sweep_stops_before_wall() {
        let grid = scene();
        let capsule = Capsule::new(0.3, 0.9);
        let from = DVec3::new(1.0, 2.2, 0.0);
        let to = DVec3::new(7.0, 2.2, 0.0);
        let contact = grid.sweep_capsule(&capsule, from, to).unwrap();
        // Расширенная стена начинается на 5.0 - (0.3 - SKIN) = 4.7 + SKIN.
        let expected = (4.7 + SKIN - 1.0) / 6.0;
        assert!((contact.time - expected).abs() < 1e-12);
        assert_eq!(contact.normal, DVec3::new(-1.0, 0.0, 0.0));
    }

    #[test]
    fn sweep_is_deterministic() {
        let grid = scene();
        let capsule = Capsule::new(0.3, 0.9);
        let from = DVec3::new(1.0, 2.2, 0.0);
        let to = DVec3::new(7.0, 2.2, 0.0);
        let first = grid.sweep_capsule(&capsule, from, to);
        let second = grid.sweep_capsule(&capsule, from, to);
        assert_eq!(first, second);
    }

    #[test]
    fn sweep_over_floor_does_not_block_horizontal_motion() {
        let grid = TileGrid::from_rows(&["........", "########"]);
        let capsule = Capsule::new(0.3, 0.9);
        // Капсула стоит ровно на полу: нижняя точка на y=1.0.
        let from = DVec3::new(2.0, 1.0 + 0.9 + 0.3, 0.0);
        let to = from + DVec3::new(1.0, 0.0, 0.0);
        assert!(grid.sweep_capsule(&capsule, from, to).is_none());
    }
}
