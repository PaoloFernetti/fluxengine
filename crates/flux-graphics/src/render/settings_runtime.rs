//! Рантайм-применение `GraphicsSettings` (ADR-0044).
//!
//! [`SettingsRuntime`] связывает настройки с живыми объектами: swapchain,
//! feature-набор кадра и поколение пермутаций. Смена пресета **не**
//! пересоздаёт Device — только те подсистемы, которые отмечены в
//! [`SettingsChange`](crate::settings::SettingsChange).
//!
//! Маппинг изменений:
//!
//! | Поле настроек | Реакция |
//! |---|---|
//! | `vsync` / `present_mode` | новый swapchain (create-then-swap) |
//! | `ssao` / `bloom` / `taa` | feature-набор кадра, граф пересобирается |
//! | `render_scale` | кадр пересоздаётся под новый размер |
//! | `shadow_resolution` | зарезервировано до появления прохода теней |
//! | `preset` | инкремент поколения пермутаций |

use super::frame::{FrameFeatures, GpuFrame};
use crate::rhi::{RhiDevice, RhiError, Swapchain, SwapchainDesc, SwapchainId};
use crate::settings::{GraphicsSettings, SettingsChange};

/// Swapchain + кадр, перестраиваемые по [`GraphicsSettings`] без нового Device.
pub struct SettingsRuntime {
    settings: GraphicsSettings,
    swapchain: Box<dyn Swapchain>,
    swapchain_desc: SwapchainDesc,
    frame: GpuFrame,
    permutation_generation: u64,
}

impl SettingsRuntime {
    /// Создаёт swapchain и кадр по настройкам.
    ///
    /// # Errors
    ///
    /// Ошибка RHI при создании swapchain, изображений или pipeline-ов.
    pub fn new(
        device: &mut dyn RhiDevice,
        desc: SwapchainDesc,
        settings: GraphicsSettings,
    ) -> Result<Self, RhiError> {
        let settings = settings.validated();
        let swapchain = device.create_swapchain(&desc.with_present_mode(settings.present_mode))?;
        let (width, height) = scaled_size(desc.width, desc.height, settings.render_scale);
        let frame = GpuFrame::with_features(
            device,
            width,
            height,
            FrameFeatures::from_settings(&settings),
        )?;
        Ok(Self {
            settings,
            swapchain,
            swapchain_desc: desc,
            frame,
            permutation_generation: 0,
        })
    }

    /// Текущие применённые настройки.
    #[must_use]
    pub fn settings(&self) -> &GraphicsSettings {
        &self.settings
    }

    /// Живой swapchain.
    #[must_use]
    pub fn swapchain(&self) -> &dyn Swapchain {
        &*self.swapchain
    }

    /// Хэндл текущего swapchain (меняется только при `SettingsChange::swapchain`).
    #[must_use]
    pub fn swapchain_id(&self) -> SwapchainId {
        self.swapchain.id()
    }

    /// Кадр GPU.
    #[must_use]
    pub fn frame(&self) -> &GpuFrame {
        &self.frame
    }

    /// Кадр GPU для записи команд.
    pub fn frame_mut(&mut self) -> &mut GpuFrame {
        &mut self.frame
    }

    /// Поколение набора пермутаций (инкремент при смене пресета).
    #[must_use]
    pub fn permutation_generation(&self) -> u64 {
        self.permutation_generation
    }

    /// Применяет новые настройки, перестраивая только затронутые подсистемы.
    ///
    /// Возвращает фактический [`SettingsChange`]. При ошибке RHI прежние
    /// настройки и ресурсы остаются в силе (create-then-swap).
    ///
    /// # Errors
    ///
    /// Ошибка RHI при создании swapchain или кадра.
    pub fn apply(
        &mut self,
        device: &mut dyn RhiDevice,
        next: GraphicsSettings,
    ) -> Result<SettingsChange, RhiError> {
        let next = next.validated();
        let change = self.settings.diff(&next);
        if !change.any() {
            self.settings = next;
            return Ok(change);
        }

        // Swapchain создаётся первым и заменяется только после успеха всех
        // остальных шагов: при ошибке прежний остаётся жив.
        let new_swapchain = if change.swapchain {
            Some(
                device
                    .create_swapchain(&self.swapchain_desc.with_present_mode(next.present_mode))?,
            )
        } else {
            None
        };

        let features = FrameFeatures::from_settings(&next);
        let size = scaled_size(
            self.swapchain_desc.width,
            self.swapchain_desc.height,
            next.render_scale,
        );
        let new_frame = if change.graph || size != self.frame.size() {
            Some(GpuFrame::with_features(device, size.0, size.1, features)?)
        } else {
            None
        };

        if let Some(frame) = new_frame {
            let old = std::mem::replace(&mut self.frame, frame);
            old.destroy(device);
        } else if change.graph {
            let _ = self.frame.set_features(device, features)?;
        }
        if let Some(swapchain) = new_swapchain {
            self.swapchain = swapchain;
        }
        if change.pipelines {
            self.permutation_generation = self.permutation_generation.wrapping_add(1);
        }
        self.settings = next;
        Ok(change)
    }

    /// Меняет размер поверхности и кадра (render scale применяется к кадру).
    ///
    /// # Errors
    ///
    /// Нулевой размер или ошибка RHI при пересоздании кадра/swapchain.
    pub fn resize(
        &mut self,
        device: &mut dyn RhiDevice,
        width: u32,
        height: u32,
    ) -> Result<(), RhiError> {
        if width == 0 || height == 0 {
            return Err(RhiError::validation("surface size must be non-zero"));
        }
        let size = scaled_size(width, height, self.settings.render_scale);
        let frame = GpuFrame::with_features(
            device,
            size.0,
            size.1,
            FrameFeatures::from_settings(&self.settings),
        )?;
        if let Err(error) = self.swapchain.resize(width, height) {
            frame.destroy(device);
            return Err(error);
        }
        let old = std::mem::replace(&mut self.frame, frame);
        old.destroy(device);
        self.swapchain_desc.width = width;
        self.swapchain_desc.height = height;
        Ok(())
    }

    /// Освобождает кадр и swapchain (Device остаётся жив).
    pub fn destroy(self, device: &mut dyn RhiDevice) {
        self.frame.destroy(device);
        drop(self.swapchain);
    }
}

/// Размер кадра с учётом render scale; минимум 1 пиксель.
fn scaled_size(width: u32, height: u32, scale: f32) -> (u32, u32) {
    let scale = if scale.is_finite() { scale } else { 1.0 };
    let scaled = |value: u32| -> u32 {
        ((value as f32 * scale).round() as i64).clamp(1, i64::from(u32::MAX)) as u32
    };
    (scaled(width), scaled(height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rhi::{Capabilities, NullDevice, PresentMode, TextureFormat};
    use crate::settings::QualityPreset;

    fn desc(width: u32, height: u32) -> SwapchainDesc {
        SwapchainDesc::new("runtime-swapchain", width, height)
            .with_format(TextureFormat::Rgba8Srgb)
            .with_frames_in_flight(2)
    }

    #[test]
    fn preset_change_rebuilds_graph_without_new_swapchain() {
        let mut device = NullDevice::new();
        let signature = device.device_signature();
        let mut runtime =
            SettingsRuntime::new(&mut device, desc(64, 64), GraphicsSettings::default()).unwrap();
        let swapchain = runtime.swapchain_id();
        assert_eq!(runtime.frame().size(), (64, 64));
        assert!(!runtime.frame().features().taa);
        assert_eq!(runtime.permutation_generation(), 0);

        let high = GraphicsSettings::from_preset(QualityPreset::High);
        let change = runtime.apply(&mut device, high).unwrap();
        assert!(change.graph && change.pipelines && !change.swapchain);
        assert_eq!(runtime.swapchain_id(), swapchain);
        assert!(runtime.frame().features().taa);
        assert_eq!(runtime.permutation_generation(), 1);
        assert_eq!(device.device_signature(), signature);

        runtime.destroy(&mut device);
    }

    #[test]
    fn present_mode_change_recreates_swapchain() {
        let mut caps = Capabilities::headless();
        caps.present_modes.push(PresentMode::Immediate);
        let mut device = NullDevice::with_capabilities(caps);
        let mut settings = GraphicsSettings::default();
        let mut runtime =
            SettingsRuntime::new(&mut device, desc(32, 32), settings.clone()).unwrap();
        let swapchain = runtime.swapchain_id();
        assert_eq!(runtime.swapchain().present_mode(), PresentMode::Fifo);

        settings.vsync = false;
        settings.present_mode = PresentMode::Immediate;
        let change = runtime.apply(&mut device, settings).unwrap();
        assert!(change.swapchain);
        assert_ne!(runtime.swapchain_id(), swapchain);
        assert_eq!(runtime.swapchain().present_mode(), PresentMode::Immediate);
        assert_eq!(runtime.settings().present_mode, PresentMode::Immediate);

        runtime.destroy(&mut device);
    }

    #[test]
    fn effect_change_keeps_swapchain_and_switches_passes() {
        let mut device = NullDevice::new();
        let mut runtime =
            SettingsRuntime::new(&mut device, desc(16, 16), GraphicsSettings::default()).unwrap();
        let swapchain = runtime.swapchain_id();

        let low = GraphicsSettings::from_preset(QualityPreset::Low);
        let change = runtime.apply(&mut device, low).unwrap();
        assert!(change.graph && !change.swapchain);
        assert_eq!(runtime.swapchain_id(), swapchain);
        assert_eq!(runtime.frame().features(), FrameFeatures::none());
        assert_eq!(runtime.frame().active_pass_count(), 3);

        runtime
            .frame_mut()
            .render(&mut device, &crate::render::FrameParams::default())
            .unwrap();
        let names: Vec<&str> = runtime
            .frame()
            .zones()
            .iter()
            .map(|zone| zone.name)
            .collect();
        assert_eq!(
            names,
            vec!["frame.gbuffer", "frame.lighting", "frame.tonemap"]
        );

        runtime.destroy(&mut device);
    }

    #[test]
    fn render_scale_changes_frame_size() {
        let mut device = NullDevice::new();
        let mut settings = GraphicsSettings::default();
        let mut runtime =
            SettingsRuntime::new(&mut device, desc(100, 50), settings.clone()).unwrap();
        assert_eq!(runtime.frame().size(), (100, 50));

        settings.render_scale = 0.5;
        let change = runtime.apply(&mut device, settings).unwrap();
        assert!(change.graph);
        assert_eq!(runtime.frame().size(), (50, 25));

        runtime.destroy(&mut device);
    }

    #[test]
    fn resize_rebuilds_frame_but_keeps_settings() {
        let mut device = NullDevice::new();
        let mut runtime =
            SettingsRuntime::new(&mut device, desc(40, 40), GraphicsSettings::default()).unwrap();
        let swapchain = runtime.swapchain_id();
        runtime.resize(&mut device, 80, 20).unwrap();
        assert_eq!(runtime.swapchain_id(), swapchain);
        assert_eq!(runtime.swapchain().size(), (80, 20));
        assert_eq!(runtime.frame().size(), (80, 20));
        assert!(runtime.resize(&mut device, 0, 20).is_err());
        runtime.destroy(&mut device);
    }

    #[test]
    fn unchanged_settings_are_a_noop() {
        let mut device = NullDevice::new();
        let mut runtime =
            SettingsRuntime::new(&mut device, desc(8, 8), GraphicsSettings::default()).unwrap();
        let change = runtime
            .apply(&mut device, GraphicsSettings::default())
            .unwrap();
        assert_eq!(change, SettingsChange::none());
        assert_eq!(runtime.permutation_generation(), 0);
        runtime.destroy(&mut device);
    }
}
