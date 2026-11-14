//! Пользовательские настройки поверх `flux-core/config`.
//!
//! Секции RON-конфига, которые меняет игрок: качество графики, громкость,
//! vsync. Настройки — отдельный слой от `EngineConfig`: конфиг движка задаёт
//! bootstrap, настройки приходят из UI и сохраняются.

use flux_core::config::ConfigSection;
use serde::{Deserialize, Serialize};

/// Пресет качества графики.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum GraphicsQuality {
    /// Минимальные эффекты, низкое разрешение теней.
    Low,
    /// Пресет по умолчанию.
    #[default]
    Medium,
    /// Высокие эффекты.
    High,
    /// Максимум, включая тяжёлые эффекты.
    Ultra,
}

impl GraphicsQuality {
    /// Разрешение карты теней в текселях.
    #[must_use]
    pub const fn shadow_resolution(self) -> u32 {
        match self {
            Self::Low => 512,
            Self::Medium => 1024,
            Self::High => 2048,
            Self::Ultra => 4096,
        }
    }

    /// Множитель разрешения рендера.
    #[must_use]
    pub const fn render_scale(self) -> f32 {
        match self {
            Self::Low => 0.75,
            Self::Medium => 1.0,
            Self::High => 1.0,
            Self::Ultra => 1.25,
        }
    }
}

/// Пользовательские настройки.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UserSettings {
    /// Пресет качества графики.
    pub graphics: GraphicsQuality,
    /// Вертикальная синхронизация.
    pub vsync: bool,
    /// Общая громкость `0.0..=1.0`.
    pub master_volume: f32,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            graphics: GraphicsQuality::default(),
            vsync: true,
            master_volume: 0.8,
        }
    }
}

impl ConfigSection for UserSettings {
    fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.master_volume) {
            return Err(format!(
                "master_volume must be within 0.0..=1.0, got {}",
                self.master_volume
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_valid() {
        assert!(UserSettings::default().validate().is_ok());
    }

    #[test]
    fn volume_out_of_range_is_rejected() {
        let settings = UserSettings {
            master_volume: 1.5,
            ..UserSettings::default()
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn quality_presets_are_ordered() {
        assert!(
            GraphicsQuality::Low.shadow_resolution() < GraphicsQuality::Medium.shadow_resolution()
        );
        assert!(
            GraphicsQuality::High.shadow_resolution() < GraphicsQuality::Ultra.shadow_resolution()
        );
    }
}
