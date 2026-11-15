//! RHI, Vulkan/DX12, Render Graph, PBR.
//!
//! Этап 4: контракты RHI и headless-null backend (I0), Vulkan с SPIR-V PSO
//! (I1/I5), Render Graph (I3), Camera/RenderScene (I4), CPU-эталоны
//! PBR/G-buffer/tonemap/post (I5), runtime-ядро шейдерных пермутаций (I6),
//! GraphicsSettings v1 (I7), golden-image-харнесс и GPU-зоны (I8).
//! GPU-бэкенды (`vulkan`, `dx12`) объявлены feature-гейтами и по умолчанию
//! выключены: headless-сборка остаётся обязательной (ADR-0005, ADR-0033,
//! ADR-0034, ADR-0035, ADR-0036).
//!
//! Канон: `docs/canon.md`. План: `docs/architecture.md`, раздел «Графика».
//! Итерации: `plans/stage-4-execution.md`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod golden;
pub mod pipeline_cache;
pub mod profile;
pub mod render;
pub mod rhi;
pub mod settings;
pub mod shaders;

pub use golden::{
    GoldenBaseline, GoldenError, GoldenSettings, GoldenStore, GoldenVerdict, Rgba8Image,
};
pub use pipeline_cache::{
    PipelineCacheFile, PipelineCacheLoad, PIPELINE_CACHE_MAGIC, PIPELINE_CACHE_VERSION,
};
pub use profile::{GpuProfiler, GpuZone};
pub use render::{
    cull_instances, decode_f16, decode_rgba16f, decode_srgb_u8, decode_unorm_u8, encode_srgb_u8,
    oct_decode, psnr_bytes, psnr_channels, reference_bloom, reference_gbuffer, reference_lighting,
    reference_ssao, reference_taa, reference_tonemap, unit_bounds, AliasSlot, Camera, CompiledPass,
    CullStats, FrameFeatures, FrameParams, FrameStage, GbufferLayout, GpuFrame, GraphError,
    GraphImage, GraphImageKey, GraphPass, InFlightBatch, Material, PassKind, ReferenceGbuffer,
    RenderGraph, RenderGraphPlan, RenderScene, ResourceEntry, ResourceKey, ResourceRegistry,
    ResourceState, SceneExtractStats, SettingsRuntime, Tonemap, TransferSystem, UploadJob,
    VramBudget, FRAME_PASS_COUNT, FRAME_PASS_NAMES, FRAME_PUSH_CONSTANT_BYTES, FRAME_QUERY_COUNT,
};
#[cfg(feature = "vulkan")]
pub use rhi::VulkanDevice;
pub use rhi::{
    create_device, BackendKind, BarrierBatch, BindingKind, BudgetSnapshot, BufferId, BufferUsage,
    Capabilities, CommandBuffer, DescriptorBinding, DescriptorResource, DescriptorSetDesc,
    DescriptorSetId, DescriptorWrite, DeviceSignature, GpuBufferDesc, GpuImageDesc, ImageBarrier,
    ImageId, ImageState, NullDevice, NullVramAllocator, PipelineDesc, PipelineId, PipelineKind,
    PresentMode, Pressure, PrimitiveTopology, RenderPassDesc, RhiDevice, RhiError, SurfaceId,
    SurfaceKind, SurfaceTarget, Swapchain, SwapchainDesc, SwapchainId, TextureFormat, TextureUsage,
    VramAllocator, VramLease, WindowSurface,
};
pub use settings::{
    FeatureKind, GraphicsFeature, GraphicsFeatureRegistry, GraphicsSettings, HardwareTier,
    QualityPreset, SettingsChange, MAX_RENDER_SCALE, MAX_SHADOW_RESOLUTION, MIN_RENDER_SCALE,
    MIN_SHADOW_RESOLUTION,
};
pub use shaders::{
    BindingDecl, CacheStats, DeterministicCompiler, KeywordDecl, KeywordMask, KeywordSet,
    PermutationCache, PermutationRequest, PipelineVariant, ResolvedShader, ShaderCompiler,
    ShaderDeclaration, ShaderError, ShaderLibrary, ShaderStage, VertexInput, MAX_KEYWORDS,
    MAX_MATERIAL_KEYWORDS, VERTEX_FORMAT_V1,
};
