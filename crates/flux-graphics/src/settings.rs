//! GraphicsSettings v1 (ADR-0016): HardwareTier → QualityPreset → гранулярность.
//!
//! Настройки — данные (RON/JSON), а не код: смена пресета перестраивает
//! Render Graph и набор пермутаций, но **никогда** не пересоздаёт Device.
//! `GraphicsFeatureRegistry` даёт авто-меню список фич с учётом
//! [`Capabilities`] конкретного бэкенда.

use serde::{Deserialize, Serialize};

use crate::rhi::{Capabilities, PresentMode};

/// Аппаратный ярус, выведенный из возможностей устройства.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum HardwareTier {
    /// Слабое устройство: минимальные эффекты.
    Low,
    /// Среднее устройство (по умолчанию).
    #[default]
    Medium,
    /// Сильное устройство.
    High,
    /// Топовое устройство с timeline semaphores.
    Ultra,
}

impl HardwareTier {
    /// Определяет ярус по возможностям бэкенда.
    ///
    /// Эвристика детерминирована и не зависит от имени GPU: размер текстур,
    /// bindless, timeline semaphores.
    #[must_use]
    pub fn from_capabilities(capabilities: &Capabilities) -> Self {
        if capabilities.max_texture_dimension_2d >= 16_384
            && capabilities.max_bindless_descriptors >= 65_536
            && capabilities.supports_timeline_semaphores
        {
            Self::Ultra
        } else if capabilities.max_texture_dimension_2d >= 16_384
            && capabilities.supports_descriptor_indexing
        {
            Self::High
        } else if capabilities.max_texture_dimension_2d >= 8_192 {
            Self::Medium
        } else {
            Self::Low
        }
    }

    /// Пресет качества по умолчанию для яруса.
    #[must_use]
    pub const fn preset(self) -> QualityPreset {
        match self {
            Self::Low => QualityPreset::Low,
            Self::Medium => QualityPreset::Medium,
            Self::High => QualityPreset::High,
            Self::Ultra => QualityPreset::Ultra,
        }
    }
}

/// Пресет качества графики.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum QualityPreset {
    /// Минимум эффектов.
    Low,
    /// Пресет по умолчанию.
    #[default]
    Medium,
    /// Высокие эффекты.
    High,
    /// Максимум, включая повышенный render scale.
    Ultra,
}

impl QualityPreset {
    /// Разрешение карты теней в текселях.
    #[must_use]
    pub const fn shadow_resolution(self) -> u32 {
        match self {
            Self::Low => 512,
            Self::Medium => 1_024,
            Self::High => 2_048,
            Self::Ultra => 4_096,
        }
    }

    /// Множитель разрешения рендера.
    #[must_use]
    pub const fn render_scale(self) -> f32 {
        match self {
            Self::Low => 0.75,
            Self::Medium | Self::High => 1.0,
            Self::Ultra => 1.25,
        }
    }

    /// Включён ли SSAO.
    #[must_use]
    pub const fn ssao(self) -> bool {
        !matches!(self, Self::Low)
    }

    /// Включён ли bloom.
    #[must_use]
    pub const fn bloom(self) -> bool {
        !matches!(self, Self::Low)
    }

    /// Включён ли TAA.
    #[must_use]
    pub const fn taa(self) -> bool {
        matches!(self, Self::High | Self::Ultra)
    }
}

/// Минимальный render scale.
pub const MIN_RENDER_SCALE: f32 = 0.25;
/// Максимальный render scale.
pub const MAX_RENDER_SCALE: f32 = 2.0;
/// Минимальное разрешение карты теней.
pub const MIN_SHADOW_RESOLUTION: u32 = 256;
/// Максимальное разрешение карты теней.
pub const MAX_SHADOW_RESOLUTION: u32 = 8_192;

/// Гранулярные настройки графики.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphicsSettings {
    /// Пресет (определяет набор keywords пермутаций).
    pub preset: QualityPreset,
    /// Разрешение карты теней, тексели.
    pub shadow_resolution: u32,
    /// Множитель разрешения рендера.
    pub render_scale: f32,
    /// SSAO.
    pub ssao: bool,
    /// Bloom.
    pub bloom: bool,
    /// TAA.
    pub taa: bool,
    /// Вертикальная синхронизация.
    pub vsync: bool,
    /// Режим представления.
    pub present_mode: PresentMode,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self::from_preset(QualityPreset::default())
    }
}

impl GraphicsSettings {
    /// Настройки пресета с включённым vsync и `Fifo`.
    #[must_use]
    pub fn from_preset(preset: QualityPreset) -> Self {
        Self {
            preset,
            shadow_resolution: preset.shadow_resolution(),
            render_scale: preset.render_scale(),
            ssao: preset.ssao(),
            bloom: preset.bloom(),
            taa: preset.taa(),
            vsync: true,
            present_mode: PresentMode::Fifo,
        }
    }

    /// Настройки для аппаратного яруса.
    #[must_use]
    pub fn from_tier(tier: HardwareTier) -> Self {
        Self::from_preset(tier.preset())
    }

    /// Ограничивает значения допустимыми диапазонами.
    pub fn sanitize(&mut self) {
        self.shadow_resolution = self
            .shadow_resolution
            .clamp(MIN_SHADOW_RESOLUTION, MAX_SHADOW_RESOLUTION);
        self.render_scale = self.render_scale.clamp(MIN_RENDER_SCALE, MAX_RENDER_SCALE);
        if self.vsync {
            self.present_mode = PresentMode::Fifo;
        }
    }

    /// Возвращает санитизированную копию.
    #[must_use]
    pub fn validated(mut self) -> Self {
        self.sanitize();
        self
    }

    /// Что нужно перестроить при переходе к `next`.
    ///
    /// Device не входит в результат: смена пресета его не пересоздаёт.
    /// `render_scale` меняет размер изображений кадра, поэтому входит в
    /// `graph` (ADR-0044).
    #[must_use]
    pub fn diff(&self, next: &Self) -> SettingsChange {
        let swapchain = self.vsync != next.vsync || self.present_mode != next.present_mode;
        let graph = self.ssao != next.ssao
            || self.bloom != next.bloom
            || self.taa != next.taa
            || self.shadow_resolution != next.shadow_resolution
            || self.render_scale != next.render_scale;
        let pipelines = self.preset != next.preset;
        SettingsChange {
            swapchain,
            graph,
            pipelines,
        }
    }

    /// Применяет новые настройки и сообщает, что перестроить.
    pub fn apply(&mut self, next: Self) -> SettingsChange {
        let change = self.diff(&next);
        *self = next.validated();
        change
    }
}

/// Реакция на смену настроек: что пересобрать.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsChange {
    /// Пересоздать swapchain (vsync/режим представления).
    pub swapchain: bool,
    /// Перестроить Render Graph (эффекты/разрешение теней).
    pub graph: bool,
    /// Пересобрать набор пермутаций (пресет).
    pub pipelines: bool,
}

impl SettingsChange {
    /// Ничего не меняется.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            swapchain: false,
            graph: false,
            pipelines: false,
        }
    }

    /// Есть ли хоть одно изменение.
    #[must_use]
    pub const fn any(self) -> bool {
        self.swapchain || self.graph || self.pipelines
    }
}

/// Вид фичи для авто-меню.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureKind {
    /// Вкл/выкл.
    Bool,
    /// Дискретные уровни.
    Level,
}

/// Фича графики, доступная в меню.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphicsFeature {
    /// Стабильное имя (ключ конфига).
    pub name: &'static str,
    /// Вид фичи.
    pub kind: FeatureKind,
    /// Число уровней (`1` для `Bool`).
    pub levels: u32,
    /// Поддерживается ли бэкендом.
    pub supported: bool,
}

/// Реестр фич для авто-меню; порядок детерминирован.
#[derive(Clone, Debug)]
pub struct GraphicsFeatureRegistry {
    features: Vec<GraphicsFeature>,
}

impl GraphicsFeatureRegistry {
    /// Строит реестр по возможностям бэкенда.
    #[must_use]
    pub fn new(capabilities: &Capabilities) -> Self {
        let features = vec![
            GraphicsFeature {
                name: "ssao",
                kind: FeatureKind::Bool,
                levels: 1,
                supported: true,
            },
            GraphicsFeature {
                name: "bloom",
                kind: FeatureKind::Bool,
                levels: 1,
                supported: true,
            },
            GraphicsFeature {
                name: "taa",
                kind: FeatureKind::Bool,
                levels: 1,
                supported: capabilities.max_frames_in_flight >= 2,
            },
            GraphicsFeature {
                name: "shadow_cascades",
                kind: FeatureKind::Level,
                levels: 4,
                supported: true,
            },
            GraphicsFeature {
                name: "bindless",
                kind: FeatureKind::Bool,
                levels: 1,
                supported: capabilities.supports_descriptor_indexing,
            },
            GraphicsFeature {
                name: "timeline_semaphores",
                kind: FeatureKind::Bool,
                levels: 1,
                supported: capabilities.supports_timeline_semaphores,
            },
            GraphicsFeature {
                name: "mailbox",
                kind: FeatureKind::Bool,
                levels: 1,
                supported: capabilities.supports_present_mode(PresentMode::Mailbox),
            },
        ];
        Self { features }
    }

    /// Все фичи в стабильном порядке.
    #[must_use]
    pub fn entries(&self) -> &[GraphicsFeature] {
        &self.features
    }

    /// Фича по имени.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&GraphicsFeature> {
        self.features.iter().find(|feature| feature.name == name)
    }

    /// Только поддерживаемые фичи.
    pub fn supported(&self) -> impl Iterator<Item = &GraphicsFeature> {
        self.features.iter().filter(|feature| feature.supported)
    }

    /// Число фич.
    #[must_use]
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Пуст ли реестр.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(
        max_texture: u32,
        bindless: u32,
        timeline: bool,
        indexing: bool,
    ) -> Capabilities {
        let mut capabilities = Capabilities::headless();
        capabilities.max_texture_dimension_2d = max_texture;
        capabilities.max_bindless_descriptors = bindless;
        capabilities.supports_timeline_semaphores = timeline;
        capabilities.supports_descriptor_indexing = indexing;
        capabilities
    }

    #[test]
    fn tiers_follow_capabilities() {
        assert_eq!(
            HardwareTier::from_capabilities(&Capabilities::headless()),
            HardwareTier::High
        );
        assert_eq!(
            HardwareTier::from_capabilities(&capabilities(16_384, 65_536, true, true)),
            HardwareTier::Ultra
        );
        assert_eq!(
            HardwareTier::from_capabilities(&capabilities(8_192, 1_024, false, false)),
            HardwareTier::Medium
        );
        assert_eq!(
            HardwareTier::from_capabilities(&capabilities(4_096, 1_024, false, false)),
            HardwareTier::Low
        );
    }

    #[test]
    fn presets_are_ordered() {
        let presets = [
            QualityPreset::Low,
            QualityPreset::Medium,
            QualityPreset::High,
            QualityPreset::Ultra,
        ];
        for pair in presets.windows(2) {
            assert!(pair[0].shadow_resolution() <= pair[1].shadow_resolution());
            assert!(pair[0].render_scale() <= pair[1].render_scale());
        }
        assert!(!QualityPreset::Low.ssao() && QualityPreset::Medium.ssao());
        assert!(!QualityPreset::Medium.taa() && QualityPreset::High.taa());
        assert_eq!(QualityPreset::Ultra.render_scale(), 1.25);
    }

    #[test]
    fn tier_maps_to_preset() {
        assert_eq!(HardwareTier::Low.preset(), QualityPreset::Low);
        assert_eq!(HardwareTier::Ultra.preset(), QualityPreset::Ultra);
        assert_eq!(
            GraphicsSettings::from_tier(HardwareTier::High).preset,
            QualityPreset::High
        );
    }

    #[test]
    fn sanitize_clamps_ranges() {
        let mut settings = GraphicsSettings::from_preset(QualityPreset::Medium);
        settings.render_scale = 10.0;
        settings.shadow_resolution = 1;
        settings.vsync = true;
        settings.present_mode = PresentMode::Immediate;
        settings.sanitize();
        assert_eq!(settings.render_scale, MAX_RENDER_SCALE);
        assert_eq!(settings.shadow_resolution, MIN_SHADOW_RESOLUTION);
        assert_eq!(settings.present_mode, PresentMode::Fifo);
    }

    #[test]
    fn diff_targets_only_affected_subsystems() {
        let base = GraphicsSettings::from_preset(QualityPreset::Medium);
        assert_eq!(base.diff(&base), SettingsChange::none());

        let mut vsync = base.clone();
        vsync.vsync = false;
        vsync.present_mode = PresentMode::Mailbox;
        let change = base.diff(&vsync);
        assert!(change.swapchain && !change.graph && !change.pipelines);

        let mut effects = base.clone();
        effects.taa = true;
        let change = base.diff(&effects);
        assert!(!change.swapchain && change.graph && !change.pipelines);

        let high = GraphicsSettings::from_preset(QualityPreset::High);
        let change = base.diff(&high);
        assert!(!change.swapchain && change.graph && change.pipelines);

        let mut scale = base.clone();
        scale.render_scale = 0.5;
        let change = base.diff(&scale);
        assert!(!change.swapchain && change.graph && !change.pipelines);
    }

    #[test]
    fn apply_returns_change_and_updates() {
        let mut settings = GraphicsSettings::default();
        let next = GraphicsSettings::from_preset(QualityPreset::Ultra);
        let change = settings.apply(next.clone());
        assert!(change.pipelines);
        assert_eq!(settings.preset, QualityPreset::Ultra);
        assert_eq!(settings.shadow_resolution, 4_096);
    }

    #[test]
    fn settings_roundtrip_through_json() {
        let settings = GraphicsSettings::from_preset(QualityPreset::High);
        let json = serde_json::to_string(&settings).expect("serialize");
        let again: GraphicsSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(settings, again);
    }

    #[test]
    fn registry_is_deterministic_and_capability_aware() {
        let capabilities = Capabilities::headless();
        let registry = GraphicsFeatureRegistry::new(&capabilities);
        assert_eq!(registry.entries().len(), 7);
        assert_eq!(registry.entries()[0].name, "ssao");
        assert!(registry.get("taa").is_some_and(|feature| feature.supported));
        assert!(!registry.get("mailbox").expect("mailbox").supported);
        assert!(
            !registry
                .get("timeline_semaphores")
                .expect("timeline")
                .supported
        );

        let mut rich = capabilities;
        rich.present_modes.push(PresentMode::Mailbox);
        rich.supports_timeline_semaphores = true;
        let rich_registry = GraphicsFeatureRegistry::new(&rich);
        assert!(rich_registry.get("mailbox").expect("mailbox").supported);
        assert_eq!(
            rich_registry.supported().count(),
            registry.supported().count() + 2
        );
    }
}
