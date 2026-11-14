//! Дисплей: размеры, DPI, мониторы, видеорежимы.
//!
//! Все типы — чистые данные без оконного API: доступны в headless-сборке
//! (ADR-0033). Конверсии logical↔physical выполняются на границе кадра.

use serde::{Deserialize, Serialize};

/// Физический размер в пикселях.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct PhysicalSize {
    /// Ширина.
    pub width: u32,
    /// Высота.
    pub height: u32,
}

impl PhysicalSize {
    /// Размер.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Площадь в пикселях.
    #[must_use]
    pub fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// В логические единицы.
    #[must_use]
    pub fn to_logical(self, dpi: DpiInfo) -> LogicalSize {
        LogicalSize {
            width: f64::from(self.width) / dpi.scale_factor,
            height: f64::from(self.height) / dpi.scale_factor,
        }
    }
}

/// Логический размер (DPI-независимые единицы).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LogicalSize {
    /// Ширина.
    pub width: f64,
    /// Высота.
    pub height: f64,
}

impl LogicalSize {
    /// Размер.
    #[must_use]
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// В физические пиксели (округление к ближайшему).
    #[must_use]
    pub fn to_physical(self, dpi: DpiInfo) -> PhysicalSize {
        PhysicalSize {
            width: round_to_u32(self.width * dpi.scale_factor),
            height: round_to_u32(self.height * dpi.scale_factor),
        }
    }
}

/// DPI-информация окна/монитора.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DpiInfo {
    /// Масштаб (1.0 — 96 DPI, 2.0 — 192 DPI).
    pub scale_factor: f64,
}

impl Default for DpiInfo {
    fn default() -> Self {
        Self { scale_factor: 1.0 }
    }
}

impl DpiInfo {
    /// Масштаб с защитой от нуля/NaN.
    #[must_use]
    pub fn new(scale_factor: f64) -> Self {
        let scale_factor = if scale_factor.is_finite() && scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };
        Self { scale_factor }
    }

    /// Физическое значение → логическое.
    #[must_use]
    pub fn physical_to_logical(self, value: f64) -> f64 {
        value / self.scale_factor
    }

    /// Логическое значение → физическое.
    #[must_use]
    pub fn logical_to_physical(self, value: f64) -> f64 {
        value * self.scale_factor
    }
}

/// Видеорежим монитора.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VideoMode {
    /// Разрешение.
    pub physical: PhysicalSize,
    /// Частота обновления в тысячных долях герца (60000 = 60 Гц).
    pub refresh_millihz: u32,
}

impl VideoMode {
    /// Режим.
    #[must_use]
    pub const fn new(physical: PhysicalSize, refresh_millihz: u32) -> Self {
        Self {
            physical,
            refresh_millihz,
        }
    }

    /// Частота обновления в герцах.
    #[must_use]
    pub fn refresh_hz(self) -> f64 {
        f64::from(self.refresh_millihz) / 1000.0
    }
}

/// Монитор.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonitorInfo {
    /// Идентификатор (стабилен в рамках сессии).
    pub id: u32,
    /// Имя для UI.
    pub name: String,
    /// Физический размер в текущем режиме.
    pub physical: PhysicalSize,
    /// DPI.
    pub dpi: DpiInfo,
    /// Основной ли монитор.
    pub primary: bool,
    /// Доступные режимы.
    pub modes: Vec<VideoMode>,
}

impl MonitorInfo {
    /// Монитор с одним режимом.
    #[must_use]
    pub fn new(
        id: u32,
        name: impl Into<String>,
        physical: PhysicalSize,
        dpi: DpiInfo,
        primary: bool,
    ) -> Self {
        let modes = vec![VideoMode::new(physical, 60_000)];
        Self {
            id,
            name: name.into(),
            physical,
            dpi,
            primary,
            modes,
        }
    }

    /// Логическое разрешение монитора.
    #[must_use]
    pub fn logical_size(&self) -> LogicalSize {
        self.physical.to_logical(self.dpi)
    }
}

fn round_to_u32(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let rounded = value.round();
    if rounded >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        rounded as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_roundtrips_logical_and_physical() {
        let dpi = DpiInfo::new(2.0);
        let logical = LogicalSize::new(640.0, 360.0);
        let physical = logical.to_physical(dpi);
        assert_eq!(physical, PhysicalSize::new(1280, 720));
        assert_eq!(physical.to_logical(dpi), logical);
    }

    #[test]
    fn invalid_dpi_falls_back_to_one() {
        assert_eq!(DpiInfo::new(0.0), DpiInfo::default());
        assert_eq!(DpiInfo::new(f64::NAN), DpiInfo::default());
    }

    #[test]
    fn video_mode_reports_hertz() {
        let mode = VideoMode::new(PhysicalSize::new(1920, 1080), 144_000);
        assert!((mode.refresh_hz() - 144.0).abs() < 1e-12);
    }

    #[test]
    fn monitor_logical_size_uses_dpi() {
        let monitor = MonitorInfo::new(
            0,
            "test",
            PhysicalSize::new(3840, 2160),
            DpiInfo::new(2.0),
            true,
        );
        assert_eq!(monitor.logical_size(), LogicalSize::new(1920.0, 1080.0));
    }
}
