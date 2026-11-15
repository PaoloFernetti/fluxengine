//! Высокоуровневый слой рендера, зависящий только от `rhi`.
//!
//! I2: `TransferSystem` (staging/батчи/ретирмент), `ResourceRegistry`
//! (состояния и bindless-слоты) и `VramBudget` (pressure-лестница).
//! I3: `RenderGraph` с автоматическими барьерами и aliasing. I4: `Camera`
//! (camera-relative, Reversed-Z) и `RenderScene`. I5: CPU-эталоны `pbr`,
//! `gbuffer`, `tonemap` и `post` для будущих GPU-проходов (I6).

pub mod budget;
pub mod frame;
pub mod gbuffer;
pub mod graph;
pub mod pbr;
pub mod post;
pub mod resources;
pub mod scene;
pub mod settings_runtime;
pub mod tonemap;
pub mod transfer;

pub use budget::{VramBudget, DEFAULT_HYSTERESIS};
pub use frame::{
    decode_f16, decode_rgba16f, decode_srgb_u8, decode_unorm_u8, encode_srgb_u8, oct_decode,
    psnr_bytes, psnr_channels, reference_bloom, reference_gbuffer, reference_lighting,
    reference_ssao, reference_taa, reference_tonemap, FrameFeatures, FrameParams, FrameStage,
    GpuFrame, ReferenceGbuffer, FRAME_PASS_COUNT, FRAME_PASS_NAMES, FRAME_PUSH_CONSTANT_BYTES,
    FRAME_QUERY_COUNT,
};
pub use gbuffer::GbufferLayout;
pub use graph::{
    AliasSlot, CompiledPass, GraphError, GraphImage, GraphImageKey, GraphPass, PassKind,
    RenderGraph, RenderGraphPlan,
};
pub use pbr::Material;
pub use resources::{ResourceEntry, ResourceKey, ResourceRegistry, ResourceState};
pub use scene::{Camera, RenderScene};
pub use settings_runtime::SettingsRuntime;
pub use tonemap::Tonemap;
pub use transfer::{InFlightBatch, TransferSystem, UploadJob, DEFAULT_STAGING_BYTES};
