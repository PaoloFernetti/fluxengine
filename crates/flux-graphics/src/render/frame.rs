//! GPU-проходы кадра (ADR-0042): G-buffer → SSAO → lighting → bloom →
//! tonemap → TAA.
//!
//! Модуль содержит две половины:
//!
//! - [`GpuFrame`] — реальные Vulkan-проходы: offscreen-изображения,
//!   pipeline-ы из закоммиченных SPIR-V, дескрипторные наборы и push
//!   constants; запись идёт через [`RenderGraph::execute_with`], поэтому
//!   барьеры и порядок пассов остаются за графом;
//! - CPU-эталоны (`reference_*`) — те же формулы на `f32` для PSNR-гейта.
//!   Эталон получает на вход **readback предыдущего прохода**, поэтому
//!   сравнение проверяет математику, а не квантование входов.
//!
//! Первый кадр TAA читает историю, очищенную в ноль; после кадра
//! изображения истории и выхода меняются местами (ping-pong).

use std::collections::BTreeMap;

use flux_math::Vec3;

use super::super::rhi::{
    BarrierBatch, BindingKind, BufferUsage, CommandBuffer, DescriptorBinding, DescriptorSetDesc,
    DescriptorSetId, DescriptorWrite, FilterMode, GpuBufferDesc, GpuImageDesc, ImageId, ImageState,
    PipelineDesc, PipelineId, RenderPassDesc, RhiDevice, RhiError, SamplerDesc, SamplerId,
    TextureFormat, TextureUsage,
};
use super::gbuffer::{GbufferLayout, COLOR_ATTACHMENTS};
use super::graph::{GraphImage, GraphImageKey, GraphPass, PassKind, RenderGraph};
use super::pbr::{self, Material};
use super::post;
use super::tonemap::Tonemap;

const FULLSCREEN_VS: &[u8] = include_bytes!("../../shaders/compiled/fullscreen.vert.spv");
const GBUFFER_FS: &[u8] = include_bytes!("../../shaders/compiled/frame_gbuffer.frag.spv");
const SSAO_FS: &[u8] = include_bytes!("../../shaders/compiled/frame_ssao.frag.spv");
const LIGHTING_FS: &[u8] = include_bytes!("../../shaders/compiled/frame_lighting.frag.spv");
const BLOOM_FS: &[u8] = include_bytes!("../../shaders/compiled/frame_bloom.frag.spv");
const TONEMAP_FS: &[u8] = include_bytes!("../../shaders/compiled/frame_tonemap.frag.spv");
const TAA_FS: &[u8] = include_bytes!("../../shaders/compiled/frame_taa.frag.spv");

/// Размер блока push constants кадра, байт (4 × float4).
pub const FRAME_PUSH_CONSTANT_BYTES: u32 = 64;

/// Параметры кадра: солнце, экспозиция и веса пост-обработки.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameParams {
    /// Направление на солнце (нормализуется в шейдере и эталоне).
    pub sun: [f32; 3],
    /// Радиантность солнца (HDR).
    pub radiance: [f32; 3],
    /// Экспозиция в стопах.
    pub exposure: f32,
    /// Порог bloom.
    pub bloom_threshold: f32,
    /// Интенсивность bloom.
    pub bloom_intensity: f32,
    /// Вес текущего кадра в TAA (`0..=1`).
    pub taa_alpha: f32,
    /// Радиус SSAO в единицах Reversed-Z.
    pub ssao_radius: f32,
}

impl Default for FrameParams {
    fn default() -> Self {
        Self {
            sun: [-0.4, 0.5, 0.75],
            radiance: [2.0, 1.9, 1.8],
            exposure: 0.0,
            bloom_threshold: 1.0,
            bloom_intensity: 0.5,
            taa_alpha: 0.5,
            ssao_radius: 0.05,
        }
    }
}

impl FrameParams {
    /// Сериализует параметры в блок push constants (4 × float4).
    #[must_use]
    pub fn push_constants(
        &self,
        width: u32,
        height: u32,
    ) -> [u8; FRAME_PUSH_CONSTANT_BYTES as usize] {
        let values = [
            1.0 / width.max(1) as f32,
            1.0 / height.max(1) as f32,
            self.exposure,
            self.bloom_threshold,
            self.sun[0],
            self.sun[1],
            self.sun[2],
            self.bloom_intensity,
            self.radiance[0],
            self.radiance[1],
            self.radiance[2],
            self.taa_alpha,
            self.ssao_radius,
            0.0,
            0.0,
            0.0,
        ];
        let mut bytes = [0u8; FRAME_PUSH_CONSTANT_BYTES as usize];
        for (index, value) in values.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}

/// Стадия кадра для readback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameStage {
    /// RT0: albedo (RGBA8 sRGB).
    Albedo,
    /// RT1: oct-normal (RGBA16F).
    Normal,
    /// RT2: ORM (RGBA8 UNORM).
    Orm,
    /// Глубина D32 (Reversed-Z).
    Depth,
    /// SSAO (RGBA8 UNORM).
    Ssao,
    /// HDR после lighting (RGBA16F).
    Hdr,
    /// Bloom (RGBA16F).
    Bloom,
    /// Tonemap (RGBA16F).
    Tonemap,
    /// TAA-выход последнего кадра (RGBA16F).
    Taa,
}

const SSAO_BINDINGS: [DescriptorBinding; 2] = [
    DescriptorBinding::new(0, BindingKind::SampledImage),
    DescriptorBinding::new(1, BindingKind::Sampler),
];

const LIGHTING_BINDINGS: [DescriptorBinding; 5] = [
    DescriptorBinding::new(0, BindingKind::SampledImage),
    DescriptorBinding::new(1, BindingKind::SampledImage),
    DescriptorBinding::new(2, BindingKind::SampledImage),
    DescriptorBinding::new(3, BindingKind::SampledImage),
    DescriptorBinding::new(4, BindingKind::Sampler),
];

const SINGLE_BINDINGS: [DescriptorBinding; 2] = [
    DescriptorBinding::new(0, BindingKind::SampledImage),
    DescriptorBinding::new(1, BindingKind::Sampler),
];

const PAIR_BINDINGS: [DescriptorBinding; 3] = [
    DescriptorBinding::new(0, BindingKind::SampledImage),
    DescriptorBinding::new(1, BindingKind::SampledImage),
    DescriptorBinding::new(2, BindingKind::Sampler),
];

struct FramePipelines {
    gbuffer: PipelineId,
    ssao: PipelineId,
    lighting: PipelineId,
    bloom: PipelineId,
    tonemap: PipelineId,
    taa: PipelineId,
}

struct FrameSets {
    ssao: DescriptorSetId,
    lighting: DescriptorSetId,
    bloom: DescriptorSetId,
    tonemap: DescriptorSetId,
    taa: DescriptorSetId,
}

/// Кадр GPU: изображения, pipeline-ы и дескрипторные наборы.
pub struct GpuFrame {
    width: u32,
    height: u32,
    rt0: ImageId,
    rt1: ImageId,
    rt2: ImageId,
    depth: ImageId,
    ssao: ImageId,
    hdr: ImageId,
    bloom: ImageId,
    tonemap: ImageId,
    taa_a: ImageId,
    taa_b: ImageId,
    history_is_a: bool,
    /// Фактические состояния изображений (для `oldLayout` барьеров).
    states: BTreeMap<GraphImageKey, ImageState>,
    sampler: SamplerId,
    pipelines: FramePipelines,
    sets: FrameSets,
}

impl GpuFrame {
    /// Создаёт изображения, sampler, pipeline-ы и наборы.
    ///
    /// # Errors
    ///
    /// Ошибка RHI: формат/размер не поддержаны, SPIR-V отвергнут или
    /// драйвер не дал память.
    pub fn new(device: &mut dyn RhiDevice, width: u32, height: u32) -> Result<Self, RhiError> {
        if width == 0 || height == 0 {
            return Err(RhiError::validation("frame size must be non-zero"));
        }
        let render_sampled = TextureUsage {
            render_target: true,
            sampled: true,
            transfer_src: true,
            ..TextureUsage::none()
        };
        let depth_usage = TextureUsage {
            depth_stencil: true,
            sampled: true,
            transfer_src: true,
            ..TextureUsage::none()
        };
        let layout = GbufferLayout::default();
        let rt0 = create_image(
            device,
            "frame-rt0",
            width,
            height,
            layout.albedo,
            render_sampled,
        )?;
        let rt1 = create_image(
            device,
            "frame-rt1",
            width,
            height,
            layout.normal,
            render_sampled,
        )?;
        let rt2 = create_image(
            device,
            "frame-rt2",
            width,
            height,
            layout.orm,
            render_sampled,
        )?;
        let depth = create_image(
            device,
            "frame-depth",
            width,
            height,
            layout.depth,
            depth_usage,
        )?;
        let ssao = create_image(
            device,
            "frame-ssao",
            width,
            height,
            TextureFormat::Rgba16Float,
            render_sampled,
        )?;
        let hdr = create_image(
            device,
            "frame-hdr",
            width,
            height,
            TextureFormat::Rgba16Float,
            render_sampled,
        )?;
        let bloom = create_image(
            device,
            "frame-bloom",
            width,
            height,
            TextureFormat::Rgba16Float,
            render_sampled,
        )?;
        let tonemap = create_image(
            device,
            "frame-tonemap",
            width,
            height,
            TextureFormat::Rgba16Float,
            render_sampled,
        )?;
        let taa_a = create_image(
            device,
            "frame-taa-a",
            width,
            height,
            TextureFormat::Rgba16Float,
            render_sampled,
        )?;
        let taa_b = create_image(
            device,
            "frame-taa-b",
            width,
            height,
            TextureFormat::Rgba16Float,
            render_sampled,
        )?;

        // История TAA обязана начинаться с нуля, а не с неопределённых данных.
        clear_image(device, taa_a, "frame-taa-a-clear")?;
        clear_image(device, taa_b, "frame-taa-b-clear")?;
        let mut states = BTreeMap::new();
        for image in [taa_a, taa_b] {
            states.insert(GraphImageKey(image.0), ImageState::ColorAttachment);
        }

        let sampler = device.create_sampler(
            &SamplerDesc::new("frame-nearest")
                .with_filters(FilterMode::Nearest, FilterMode::Nearest),
        )?;

        let push = FRAME_PUSH_CONSTANT_BYTES;
        let gbuffer = device.create_pipeline(
            &PipelineDesc::graphics("frame-gbuffer", FULLSCREEN_VS, GBUFFER_FS)
                .with_color_formats(&GBUFFER_FORMATS)
                .with_depth_format(layout.depth)
                .with_vertex_stride(0)
                .with_push_constants(push),
        )?;
        let ssao_pipeline = device.create_pipeline(
            &PipelineDesc::graphics("frame-ssao", FULLSCREEN_VS, SSAO_FS)
                .with_color_formats(&[TextureFormat::Rgba16Float])
                .with_vertex_stride(0)
                .with_bindings(&SSAO_BINDINGS)
                .with_push_constants(push),
        )?;
        let lighting = device.create_pipeline(
            &PipelineDesc::graphics("frame-lighting", FULLSCREEN_VS, LIGHTING_FS)
                .with_color_formats(&[TextureFormat::Rgba16Float])
                .with_vertex_stride(0)
                .with_bindings(&LIGHTING_BINDINGS)
                .with_push_constants(push),
        )?;
        let bloom_pipeline = device.create_pipeline(
            &PipelineDesc::graphics("frame-bloom", FULLSCREEN_VS, BLOOM_FS)
                .with_color_formats(&[TextureFormat::Rgba16Float])
                .with_vertex_stride(0)
                .with_bindings(&SINGLE_BINDINGS)
                .with_push_constants(push),
        )?;
        let tonemap_pipeline = device.create_pipeline(
            &PipelineDesc::graphics("frame-tonemap", FULLSCREEN_VS, TONEMAP_FS)
                .with_color_formats(&[TextureFormat::Rgba16Float])
                .with_vertex_stride(0)
                .with_bindings(&PAIR_BINDINGS)
                .with_push_constants(push),
        )?;
        let taa_pipeline = device.create_pipeline(
            &PipelineDesc::graphics("frame-taa", FULLSCREEN_VS, TAA_FS)
                .with_color_formats(&[TextureFormat::Rgba16Float])
                .with_vertex_stride(0)
                .with_bindings(&PAIR_BINDINGS)
                .with_push_constants(push),
        )?;

        let ssao_set = device.create_descriptor_set(
            &DescriptorSetDesc::new("frame-ssao").with_bindings(&SSAO_BINDINGS),
        )?;
        device.update_descriptor_set(
            ssao_set,
            &[
                DescriptorWrite::image(0, 0, depth),
                DescriptorWrite::sampler(1, 0, sampler),
            ],
        )?;
        let lighting_set = device.create_descriptor_set(
            &DescriptorSetDesc::new("frame-lighting").with_bindings(&LIGHTING_BINDINGS),
        )?;
        device.update_descriptor_set(
            lighting_set,
            &[
                DescriptorWrite::image(0, 0, rt0),
                DescriptorWrite::image(1, 0, rt1),
                DescriptorWrite::image(2, 0, rt2),
                DescriptorWrite::image(3, 0, ssao),
                DescriptorWrite::sampler(4, 0, sampler),
            ],
        )?;
        let bloom_set = device.create_descriptor_set(
            &DescriptorSetDesc::new("frame-bloom").with_bindings(&SINGLE_BINDINGS),
        )?;
        device.update_descriptor_set(
            bloom_set,
            &[
                DescriptorWrite::image(0, 0, hdr),
                DescriptorWrite::sampler(1, 0, sampler),
            ],
        )?;
        let tonemap_set = device.create_descriptor_set(
            &DescriptorSetDesc::new("frame-tonemap").with_bindings(&PAIR_BINDINGS),
        )?;
        device.update_descriptor_set(
            tonemap_set,
            &[
                DescriptorWrite::image(0, 0, hdr),
                DescriptorWrite::image(1, 0, bloom),
                DescriptorWrite::sampler(2, 0, sampler),
            ],
        )?;
        let taa_set = device.create_descriptor_set(
            &DescriptorSetDesc::new("frame-taa").with_bindings(&PAIR_BINDINGS),
        )?;
        // История — taa_a в первом кадре; после рендера изображения меняются.
        device.update_descriptor_set(
            taa_set,
            &[
                DescriptorWrite::image(0, 0, tonemap),
                DescriptorWrite::image(1, 0, taa_a),
                DescriptorWrite::sampler(2, 0, sampler),
            ],
        )?;

        Ok(Self {
            width,
            height,
            rt0,
            rt1,
            rt2,
            depth,
            ssao,
            hdr,
            bloom,
            tonemap,
            taa_a,
            taa_b,
            history_is_a: true,
            states,
            sampler,
            pipelines: FramePipelines {
                gbuffer,
                ssao: ssao_pipeline,
                lighting,
                bloom: bloom_pipeline,
                tonemap: tonemap_pipeline,
                taa: taa_pipeline,
            },
            sets: FrameSets {
                ssao: ssao_set,
                lighting: lighting_set,
                bloom: bloom_set,
                tonemap: tonemap_set,
                taa: taa_set,
            },
        })
    }

    /// Размер кадра.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Изображение и его состояние после [`Self::render`], а также байт на
    /// пиксель — для readback.
    #[must_use]
    pub fn stage(&self, stage: FrameStage) -> (ImageId, ImageState, u32) {
        match stage {
            FrameStage::Albedo => (self.rt0, ImageState::ShaderRead, 4),
            FrameStage::Normal => (self.rt1, ImageState::ShaderRead, 8),
            FrameStage::Orm => (self.rt2, ImageState::ShaderRead, 4),
            FrameStage::Depth => (self.depth, ImageState::ShaderRead, 4),
            FrameStage::Ssao => (self.ssao, ImageState::ShaderRead, 8),
            FrameStage::Hdr => (self.hdr, ImageState::ShaderRead, 8),
            FrameStage::Bloom => (self.bloom, ImageState::ShaderRead, 8),
            FrameStage::Tonemap => (self.tonemap, ImageState::ShaderRead, 8),
            FrameStage::Taa => {
                // После рендера история указывает на последний записанный выход.
                let output = if self.history_is_a {
                    self.taa_a
                } else {
                    self.taa_b
                };
                (output, ImageState::ColorAttachment, 8)
            }
        }
    }

    /// Рисует кадр: барьеры и тела пассов из Render Graph.
    ///
    /// # Errors
    ///
    /// Ошибка командного буфера или графического прохода.
    pub fn render(
        &mut self,
        device: &mut dyn RhiDevice,
        params: &FrameParams,
    ) -> Result<(), RhiError> {
        let (history, output) = if self.history_is_a {
            (self.taa_a, self.taa_b)
        } else {
            (self.taa_b, self.taa_a)
        };
        let graph = self.frame_graph(history, output)?;
        let plan = graph
            .compile_with_initial(&self.states)
            .map_err(|error| RhiError::validation(format!("frame graph: {error}")))?;
        let push = params.push_constants(self.width, self.height);

        let mut commands = device.create_command_buffer()?;
        commands.begin()?;
        let (rt0, rt1, rt2, depth) = (self.rt0, self.rt1, self.rt2, self.depth);
        let (ssao, hdr, bloom, tonemap) = (self.ssao, self.hdr, self.bloom, self.tonemap);
        let pipelines = &self.pipelines;
        let sets = &self.sets;
        graph.execute_with(&plan, &mut *commands, |pass, commands| match pass.name {
            "frame.gbuffer" => {
                let desc = RenderPassDesc::new("frame-gbuffer")
                    .with_color(rt0)
                    .with_color(rt1)
                    .with_color(rt2)
                    .with_depth(depth)
                    .with_clear([0.0, 0.0, 0.0, 1.0]);
                draw(commands, pipelines.gbuffer, None, &push, &desc)
            }
            "frame.ssao" => {
                let desc = RenderPassDesc::new("frame-ssao")
                    .with_color(ssao)
                    .with_clear([1.0, 1.0, 1.0, 1.0]);
                draw(commands, pipelines.ssao, Some(sets.ssao), &push, &desc)
            }
            "frame.lighting" => {
                let desc = RenderPassDesc::new("frame-lighting")
                    .with_color(hdr)
                    .with_clear([0.0, 0.0, 0.0, 1.0]);
                draw(
                    commands,
                    pipelines.lighting,
                    Some(sets.lighting),
                    &push,
                    &desc,
                )
            }
            "frame.bloom" => {
                let desc = RenderPassDesc::new("frame-bloom")
                    .with_color(bloom)
                    .with_clear([0.0, 0.0, 0.0, 1.0]);
                draw(commands, pipelines.bloom, Some(sets.bloom), &push, &desc)
            }
            "frame.tonemap" => {
                let desc = RenderPassDesc::new("frame-tonemap")
                    .with_color(tonemap)
                    .with_clear([0.0, 0.0, 0.0, 1.0]);
                draw(
                    commands,
                    pipelines.tonemap,
                    Some(sets.tonemap),
                    &push,
                    &desc,
                )
            }
            "frame.taa" => {
                let desc = RenderPassDesc::new("frame-taa")
                    .with_color(output)
                    .with_clear([0.0, 0.0, 0.0, 1.0]);
                draw(commands, pipelines.taa, Some(sets.taa), &push, &desc)
            }
            other => Err(RhiError::validation(format!("unknown frame pass: {other}"))),
        })?;
        commands.end()?;
        commands.finish()?;
        device.wait_idle()?;

        // Ping-pong: следующий кадр читает только что записанный выход.
        self.history_is_a = !self.history_is_a;
        // Набор TAA теперь читает новый выход, а пишет бывшую историю.
        let (next_history, next_output) = if self.history_is_a {
            (self.taa_a, self.taa_b)
        } else {
            (self.taa_b, self.taa_a)
        };
        debug_assert_eq!(next_history, output);
        debug_assert_eq!(next_output, history);
        // Фактические состояния после кадра: все чтения — ShaderRead,
        // последний выход — ColorAttachment.
        self.states.clear();
        for image in [
            self.rt0,
            self.rt1,
            self.rt2,
            self.depth,
            self.ssao,
            self.hdr,
            self.bloom,
            self.tonemap,
            history,
        ] {
            self.states
                .insert(GraphImageKey(image.0), ImageState::ShaderRead);
        }
        self.states
            .insert(GraphImageKey(output.0), ImageState::ColorAttachment);
        device.update_descriptor_set(
            self.sets.taa,
            &[
                DescriptorWrite::image(0, 0, self.tonemap),
                DescriptorWrite::image(1, 0, next_history),
                DescriptorWrite::sampler(2, 0, self.sampler),
            ],
        )?;
        Ok(())
    }

    /// Читает стадию кадра в staging-буфер.
    ///
    /// # Errors
    ///
    /// Ошибка RHI: неизвестная стадия, барьер или копирование.
    pub fn readback(
        &mut self,
        device: &mut dyn RhiDevice,
        stage: FrameStage,
    ) -> Result<Vec<u8>, RhiError> {
        let (image, from, bytes_per_pixel) = self.stage(stage);
        let bytes = u64::from(self.width)
            .saturating_mul(u64::from(self.height))
            .saturating_mul(u64::from(bytes_per_pixel));
        let buffer = device.create_buffer(&GpuBufferDesc::new(
            "frame-readback",
            bytes,
            BufferUsage::staging(),
        ))?;
        let mut commands = device.create_command_buffer()?;
        commands.begin()?;
        let mut batch = BarrierBatch::new();
        batch.transition(image, from, ImageState::TransferSrc);
        commands.barriers(&batch)?;
        commands.copy_image_to_buffer(image, buffer)?;
        commands.end()?;
        commands.finish()?;
        device.wait_idle()?;
        let data = device.read_buffer(buffer)?;
        let _ = device.destroy_buffer(buffer);
        self.states
            .insert(GraphImageKey(image.0), ImageState::TransferSrc);
        Ok(data)
    }

    /// Освобождает изображения, pipeline-ы, sampler и наборы.
    pub fn destroy(self, device: &mut dyn RhiDevice) {
        for pipeline in [
            self.pipelines.gbuffer,
            self.pipelines.ssao,
            self.pipelines.lighting,
            self.pipelines.bloom,
            self.pipelines.tonemap,
            self.pipelines.taa,
        ] {
            let _ = device.destroy_pipeline(pipeline);
        }
        for set in [
            self.sets.ssao,
            self.sets.lighting,
            self.sets.bloom,
            self.sets.tonemap,
            self.sets.taa,
        ] {
            let _ = device.destroy_descriptor_set(set);
        }
        let _ = device.destroy_sampler(self.sampler);
        for image in [
            self.rt0,
            self.rt1,
            self.rt2,
            self.depth,
            self.ssao,
            self.hdr,
            self.bloom,
            self.tonemap,
            self.taa_a,
            self.taa_b,
        ] {
            let _ = device.destroy_image(image);
        }
    }

    fn frame_graph(&self, history: ImageId, output: ImageId) -> Result<RenderGraph, RhiError> {
        let key = |image: ImageId| GraphImageKey(image.0);
        let mut graph = RenderGraph::new();
        graph.set_output_size(self.width, self.height);
        let images = [
            self.rt0,
            self.rt1,
            self.rt2,
            self.depth,
            self.ssao,
            self.hdr,
            self.bloom,
            self.tonemap,
            history,
            output,
        ];
        for image in images {
            graph
                .add_image(GraphImage::external(key(image), 0))
                .map_err(|error| RhiError::validation(format!("frame graph: {error}")))?;
        }
        let mut add = |pass: GraphPass| -> Result<(), RhiError> {
            graph
                .add_pass(pass)
                .map_err(|error| RhiError::validation(format!("frame graph: {error}")))
        };
        add(GraphPass::new("frame.gbuffer", PassKind::Graphics)
            .color(key(self.rt0))
            .color(key(self.rt1))
            .color(key(self.rt2))
            .depth(key(self.depth)))?;
        add(GraphPass::new("frame.ssao", PassKind::Graphics)
            .read(key(self.depth))
            .color(key(self.ssao)))?;
        add(GraphPass::new("frame.lighting", PassKind::Graphics)
            .read(key(self.rt0))
            .read(key(self.rt1))
            .read(key(self.rt2))
            .read(key(self.ssao))
            .color(key(self.hdr)))?;
        add(GraphPass::new("frame.bloom", PassKind::Graphics)
            .read(key(self.hdr))
            .color(key(self.bloom)))?;
        add(GraphPass::new("frame.tonemap", PassKind::Graphics)
            .read(key(self.hdr))
            .read(key(self.bloom))
            .color(key(self.tonemap)))?;
        add(GraphPass::new("frame.taa", PassKind::Graphics)
            .read(key(self.tonemap))
            .read(key(history))
            .color(key(output)))?;
        Ok(graph)
    }
}

fn create_image(
    device: &mut dyn RhiDevice,
    label: &'static str,
    width: u32,
    height: u32,
    format: TextureFormat,
    usage: TextureUsage,
) -> Result<ImageId, RhiError> {
    device.create_image(&GpuImageDesc::new(label, width, height, format).with_usage(usage))
}

fn clear_image(
    device: &mut dyn RhiDevice,
    image: ImageId,
    label: &'static str,
) -> Result<(), RhiError> {
    let mut commands = device.create_command_buffer()?;
    commands.begin()?;
    let mut batch = BarrierBatch::new();
    batch.transition(image, ImageState::Undefined, ImageState::ColorAttachment);
    commands.barriers(&batch)?;
    let pass = RenderPassDesc::new(label)
        .with_color(image)
        .with_clear([0.0, 0.0, 0.0, 1.0]);
    commands.begin_render_pass(&pass)?;
    commands.end_render_pass()?;
    commands.end()?;
    commands.finish()
}

fn draw(
    commands: &mut dyn CommandBuffer,
    pipeline: PipelineId,
    set: Option<DescriptorSetId>,
    push: &[u8],
    pass: &RenderPassDesc,
) -> Result<(), RhiError> {
    commands.begin_render_pass(pass)?;
    commands.set_pipeline(pipeline)?;
    if let Some(set) = set {
        commands.bind_descriptor_set(set)?;
    }
    commands.push_constants(push)?;
    commands.draw(3, 1)?;
    commands.end_render_pass()
}

/// Форматы G-buffer-пайплайна в порядке RT0..RT2 (ADR-0017).
const GBUFFER_FORMATS: [TextureFormat; COLOR_ATTACHMENTS] = [
    TextureFormat::Rgba8Srgb,
    TextureFormat::Rgba16Float,
    TextureFormat::Rgba8Unorm,
];

// ---------------------------------------------------------------------------
// CPU-эталоны: те же формулы на f32.
// ---------------------------------------------------------------------------

/// Выход CPU-эталона G-buffer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReferenceGbuffer {
    /// Линейный albedo.
    pub albedo: Vec<Vec3>,
    /// Нормали (единичные).
    pub normal: Vec<Vec3>,
    /// Шероховатость.
    pub roughness: Vec<f32>,
    /// Металличность.
    pub metallic: Vec<f32>,
    /// Начальная AO.
    pub ao_seed: Vec<f32>,
    /// Глубина Reversed-Z.
    pub depth: Vec<f32>,
}

/// CPU-эталон G-buffer-прохода.
#[must_use]
pub fn reference_gbuffer(width: u32, height: u32) -> ReferenceGbuffer {
    let count = (width as usize) * (height as usize);
    let mut frame = ReferenceGbuffer {
        albedo: Vec::with_capacity(count),
        normal: Vec::with_capacity(count),
        roughness: Vec::with_capacity(count),
        metallic: Vec::with_capacity(count),
        ao_seed: Vec::with_capacity(count),
        depth: Vec::with_capacity(count),
    };
    for y in 0..height {
        for x in 0..width {
            let uv_x = (x as f32 + 0.5) / width as f32;
            let uv_y = (y as f32 + 0.5) / height as f32;
            let p = Vec3::new(uv_x * 2.0 - 1.0, uv_y * 2.0 - 1.0, 0.0);
            let h = (1.0 - p.dot(p)).clamp(0.0, 1.0);
            let normal = Vec3::new(-p.x * 2.0, -p.y * 2.0, 0.5 + h).normalize();
            let albedo = Vec3::new(0.15, 0.2, 0.25).lerp(Vec3::new(0.85, 0.55, 0.3), h);
            frame.albedo.push(albedo);
            frame.normal.push(normal);
            frame.roughness.push(0.85 + (0.25 - 0.85) * h);
            frame.metallic.push((h * 1.5 - 0.5).clamp(0.0, 1.0) * 0.9);
            frame.ao_seed.push((0.6 + 0.4 * h).clamp(0.0, 1.0));
            frame.depth.push(1.0 - h * 0.5);
        }
    }
    frame
}

/// CPU-эталон SSAO: 4 соседа, `post::ssao_factor`.
#[must_use]
pub fn reference_ssao(width: u32, height: u32, depth: &[f32], radius: f32) -> Vec<f32> {
    let mut result = Vec::with_capacity(depth.len());
    for y in 0..height {
        for x in 0..width {
            let center = depth[(y * width + x) as usize];
            let neighbors = [
                (x.saturating_add(1).min(width - 1), y),
                (x.saturating_sub(1), y),
                (x, y.saturating_add(1).min(height - 1)),
                (x, y.saturating_sub(1)),
            ];
            let samples: Vec<f32> = neighbors
                .iter()
                .map(|(nx, ny)| depth[(ny * width + nx) as usize])
                .collect();
            result.push(post::ssao_factor(center, &samples, radius));
        }
    }
    result
}

/// CPU-эталон deferred lighting (GGX/Smith/Fresnel/multiscatter).
#[must_use]
pub fn reference_lighting(
    width: u32,
    height: u32,
    albedo_srgb: &[u8],
    normal_oct: &[f32],
    orm: &[u8],
    ao: &[f32],
    params: &FrameParams,
) -> Vec<Vec3> {
    let view = Vec3::new(0.0, 0.0, 1.0);
    let light = Vec3::from_array(params.sun).normalize();
    let radiance = Vec3::from_array(params.radiance);
    let count = (width as usize) * (height as usize);
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        let base = index * 4;
        let albedo = Vec3::new(
            decode_srgb_u8(albedo_srgb[base]),
            decode_srgb_u8(albedo_srgb[base + 1]),
            decode_srgb_u8(albedo_srgb[base + 2]),
        );
        let normal = oct_decode(normal_oct[index * 2], normal_oct[index * 2 + 1]);
        let roughness = decode_unorm_u8(orm[base + 1]);
        let metallic = decode_unorm_u8(orm[base + 2]);
        let ao = ao[index].clamp(0.0, 1.0);
        let material = Material::new(albedo, metallic, roughness);
        result.push(pbr::evaluate_direct(material, normal, view, light, radiance) * ao);
    }
    result
}

/// CPU-эталон bloom: порог и box-блюр 5 отсчётов.
#[must_use]
pub fn reference_bloom(width: u32, height: u32, hdr: &[Vec3], threshold: f32) -> Vec<Vec3> {
    let mut result = Vec::with_capacity(hdr.len());
    for y in 0..height {
        for x in 0..width {
            let mut sum = post::bloom_threshold(hdr[(y * width + x) as usize], threshold);
            let neighbors = [
                (x.saturating_add(1).min(width - 1), y),
                (x.saturating_sub(1), y),
                (x, y.saturating_add(1).min(height - 1)),
                (x, y.saturating_sub(1)),
            ];
            for (nx, ny) in neighbors {
                sum += post::bloom_threshold(hdr[(ny * width + nx) as usize], threshold);
            }
            result.push(sum / 5.0);
        }
    }
    result
}

/// CPU-эталон tonemap: combine + ACES.
#[must_use]
pub fn reference_tonemap(hdr: &[Vec3], bloom: &[Vec3], params: &FrameParams) -> Vec<Vec3> {
    hdr.iter()
        .zip(bloom)
        .map(|(&base, &glow)| {
            let combined = post::bloom_combine(base, glow, params.bloom_intensity);
            Tonemap::Aces.apply(combined, params.exposure)
        })
        .collect()
}

/// CPU-эталон TAA: смешивание истории и текущего кадра.
#[must_use]
pub fn reference_taa(current: &[Vec3], history: &[Vec3], alpha: f32) -> Vec<Vec3> {
    current
        .iter()
        .zip(history)
        .map(|(&now, &before)| post::taa_blend(before, now, alpha))
        .collect()
}

/// Декодирует 8-битный sRGB в линейное значение.
#[must_use]
pub fn decode_srgb_u8(byte: u8) -> f32 {
    let value = f32::from(byte) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Декодирует 8-битный UNORM.
#[must_use]
pub fn decode_unorm_u8(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

/// Кодирует линейное значение в 8-битный sRGB (round-to-nearest).
#[must_use]
pub fn encode_srgb_u8(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0 + 0.5).floor() as u8
}

/// Декодирует IEEE-754 binary16.
#[must_use]
pub fn decode_f16(bits: u16) -> f32 {
    let sign = u32::from(bits >> 15);
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = u32::from(bits & 0x03ff);
    let value = if exponent == 0 {
        (fraction as f32) * 2f32.powi(-24)
    } else if exponent == 0x1f {
        if fraction == 0 {
            f32::INFINITY
        } else {
            f32::NAN
        }
    } else {
        (1.0 + fraction as f32 / 1024.0) * 2f32.powi(exponent - 15)
    };
    if sign == 1 {
        -value
    } else {
        value
    }
}

/// Читает 4 компонента RGBA16F из readback.
#[must_use]
pub fn decode_rgba16f(bytes: &[u8], index: usize) -> [f32; 4] {
    let mut result = [0.0f32; 4];
    for (channel, slot) in result.iter_mut().enumerate() {
        let offset = index * 8 + channel * 2;
        let bits = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        *slot = decode_f16(bits);
    }
    result
}

/// Декодирует oct-normal.
#[must_use]
pub fn oct_decode(x: f32, y: f32) -> Vec3 {
    let mut normal = Vec3::new(x, y, 1.0 - x.abs() - y.abs());
    if normal.z < 0.0 {
        let (nx, ny) = (normal.x, normal.y);
        normal.x = (1.0 - ny.abs()) * if nx >= 0.0 { 1.0 } else { -1.0 };
        normal.y = (1.0 - nx.abs()) * if ny >= 0.0 { 1.0 } else { -1.0 };
    }
    normal.normalize()
}

/// PSNR для набора скалярных каналов (peak — максимальное значение эталона).
#[must_use]
pub fn psnr_channels(reference: &[f32], actual: &[f32], peak: f32) -> f64 {
    assert_eq!(reference.len(), actual.len(), "psnr: length mismatch");
    if reference.is_empty() {
        return f64::INFINITY;
    }
    let mse = reference
        .iter()
        .zip(actual)
        .map(|(&expected, &got)| {
            let delta = f64::from(expected) - f64::from(got);
            delta * delta
        })
        .sum::<f64>()
        / reference.len() as f64;
    if mse <= f64::EPSILON {
        f64::INFINITY
    } else {
        10.0 * (f64::from(peak) * f64::from(peak) / mse).log10()
    }
}

/// PSNR для байтовых изображений (peak 255).
#[must_use]
pub fn psnr_bytes(reference: &[u8], actual: &[u8]) -> f64 {
    assert_eq!(reference.len(), actual.len(), "psnr: length mismatch");
    if reference.is_empty() {
        return f64::INFINITY;
    }
    let mse = reference
        .iter()
        .zip(actual)
        .map(|(&expected, &got)| {
            let delta = f64::from(expected) - f64::from(got);
            delta * delta
        })
        .sum::<f64>()
        / reference.len() as f64;
    if mse <= f64::EPSILON {
        f64::INFINITY
    } else {
        10.0 * (255.0 * 255.0 / mse).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_constants_are_16_floats() {
        let params = FrameParams::default();
        let bytes = params.push_constants(64, 32);
        assert_eq!(bytes.len(), FRAME_PUSH_CONSTANT_BYTES as usize);
        let first = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert!((first - 1.0 / 64.0).abs() < 1e-6);
    }

    #[test]
    fn f16_decoder_matches_known_values() {
        assert_eq!(decode_f16(0x3c00), 1.0);
        assert_eq!(decode_f16(0x0000), 0.0);
        assert_eq!(decode_f16(0xbc00), -1.0);
        assert!((decode_f16(0x3800) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn oct_roundtrip_is_stable() {
        for normal in [
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.6, -0.3, 0.74).normalize(),
        ] {
            let encoded = oct_encode_reference(normal);
            let decoded = oct_decode(encoded.0, encoded.1);
            assert!((decoded - normal).length() < 1e-5);
        }
    }

    #[test]
    fn reference_gbuffer_is_finite() {
        let frame = reference_gbuffer(8, 8);
        assert_eq!(frame.albedo.len(), 64);
        assert!(frame.depth.iter().all(|value| (0.0..=1.0).contains(value)));
        assert!(frame
            .normal
            .iter()
            .all(|normal| (normal.length() - 1.0).abs() < 1e-5));
    }

    #[test]
    fn psnr_is_infinite_for_identical_data() {
        let data = [0.1f32, 0.5, 0.9];
        assert!(psnr_channels(&data, &data, 1.0).is_infinite());
        assert!(psnr_bytes(&[1, 2, 3], &[1, 2, 3]).is_infinite());
    }

    fn oct_encode_reference(normal: Vec3) -> (f32, f32) {
        let n = normal / (normal.x.abs() + normal.y.abs() + normal.z.abs());
        if n.z >= 0.0 {
            (n.x, n.y)
        } else {
            let sx = if n.x >= 0.0 { 1.0 } else { -1.0 };
            let sy = if n.y >= 0.0 { 1.0 } else { -1.0 };
            ((1.0 - n.y.abs()) * sx, (1.0 - n.x.abs()) * sy)
        }
    }
}
