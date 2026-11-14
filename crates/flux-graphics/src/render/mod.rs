//! Высокоуровневый слой рендера, зависящий только от `rhi`.
//!
//! I2: `TransferSystem` (staging/батчи/ретирмент), `ResourceRegistry`
//! (состояния и bindless-слоты) и `VramBudget` (pressure-лестница).
//! I3: `RenderGraph` с автоматическими барьерами и aliasing. I4: `Camera`
//! (camera-relative, Reversed-Z) и `RenderScene`. I5: CPU-эталоны `pbr`,
//! `gbuffer`, `tonemap` и `post` для будущих GPU-проходов (I6).

pub mod budget;
pub mod gbuffer;
pub mod graph;
pub mod pbr;
pub mod post;
pub mod resources;
pub mod scene;
pub mod tonemap;
pub mod transfer;

pub use budget::{VramBudget, DEFAULT_HYSTERESIS};
pub use gbuffer::GbufferLayout;
pub use graph::{
    AliasSlot, CompiledPass, GraphError, GraphImage, GraphImageKey, GraphPass, PassKind,
    RenderGraph, RenderGraphPlan,
};
pub use pbr::Material;
pub use resources::{ResourceEntry, ResourceKey, ResourceRegistry, ResourceState};
pub use scene::{Camera, RenderScene};
pub use tonemap::Tonemap;
pub use transfer::{InFlightBatch, TransferSystem, UploadJob, DEFAULT_STAGING_BYTES};
