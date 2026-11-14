//! Headless-null backend: детерминированные хэндлы, учёт VRAM, проверки вызовов.
//!
//! Null не выполняет GPU-команд, но соблюдает контракты RHI. Это позволяет
//! тестировать Render Graph, TransferSystem и бюджет без GPU (ADR-0005).

use std::collections::{BTreeMap, BTreeSet};

use flux_core::memory::MemoryTag;

use super::{
    cmd::{BarrierBatch, CommandBuffer},
    device::RhiDevice,
    error::RhiError,
    swapchain::Swapchain,
    types::{
        BackendKind, BindingKind, BufferId, BufferUsage, Capabilities, DescriptorBinding,
        DescriptorResource, DescriptorSetDesc, DescriptorSetId, DescriptorWrite, GpuBufferDesc,
        GpuImageDesc, ImageId, PipelineDesc, PipelineId, PresentMode, QueryPoolDesc, QueryPoolId,
        RenderPassDesc, SwapchainDesc, SwapchainId, TextureFormat,
    },
    vram::{BudgetSnapshot, Pressure, VramAllocator, VramLease},
};

/// Host-visible буфер: staging-пара (transfer src+dst) без GPU-использования.
fn is_host_visible(usage: BufferUsage) -> bool {
    usage.transfer_src
        && usage.transfer_dst
        && !(usage.vertex || usage.index || usage.uniform || usage.storage || usage.indirect)
}

/// Дескрипторный набор null-устройства: layout и записанные ресурсы.
#[derive(Clone, Debug)]
struct NullDescriptorSet {
    bindings: Vec<DescriptorBinding>,
    writes: BTreeMap<(u32, u32), DescriptorResource>,
}

const DEFAULT_VRAM_BUDGET: u64 = 256 * 1024 * 1024;

/// Аллокатор VRAM для null-бэкенда.
#[derive(Clone, Debug)]
pub struct NullVramAllocator {
    total: u64,
    used: u64,
    next_lease: u64,
    leases: BTreeMap<u64, VramLease>,
}

impl NullVramAllocator {
    /// Аллокатор с бюджетом `total` байт.
    #[must_use]
    pub fn new(total: u64) -> Self {
        Self {
            total,
            used: 0,
            next_lease: 0,
            leases: BTreeMap::new(),
        }
    }

    /// Всего байт.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Занято байт.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.used
    }

    /// Число активных лизингов.
    #[must_use]
    pub fn lease_count(&self) -> usize {
        self.leases.len()
    }
}

impl Default for NullVramAllocator {
    fn default() -> Self {
        Self::new(DEFAULT_VRAM_BUDGET)
    }
}

impl VramAllocator for NullVramAllocator {
    fn allocate(&mut self, bytes: u64, tag: MemoryTag) -> Result<VramLease, RhiError> {
        if bytes == 0 {
            return Err(RhiError::validation("zero-sized VRAM allocation"));
        }
        let end = self
            .used
            .checked_add(bytes)
            .ok_or(RhiError::OutOfMemory { requested: bytes })?;
        if end > self.total {
            return Err(RhiError::OutOfMemory { requested: bytes });
        }
        let lease = VramLease {
            id: self.next_lease,
            bytes,
            tag,
        };
        self.next_lease = self.next_lease.wrapping_add(1);
        self.used = end;
        self.leases.insert(lease.id, lease);
        Ok(lease)
    }

    fn free(&mut self, lease: VramLease) -> bool {
        let Some(removed) = self.leases.remove(&lease.id) else {
            return false;
        };
        self.used = self.used.saturating_sub(removed.bytes);
        true
    }

    fn budget(&self) -> BudgetSnapshot {
        let snapshot = BudgetSnapshot {
            total: self.total,
            reserved: 0,
            used: self.used,
            pressure: Pressure::Normal,
        };
        BudgetSnapshot {
            pressure: Pressure::from_ratio(snapshot.utilization()),
            ..snapshot
        }
    }
}

/// Null-устройство.
pub struct NullDevice {
    caps: Capabilities,
    next_buffer: u64,
    next_image: u64,
    next_pipeline: u64,
    next_swapchain: u64,
    next_query_pool: u64,
    next_descriptor_set: u64,
    buffers: BTreeMap<u64, GpuBufferDesc>,
    images: BTreeMap<u64, GpuImageDesc>,
    pipelines: BTreeMap<u64, PipelineDesc>,
    query_pools: BTreeMap<u64, u32>,
    descriptor_sets: BTreeMap<u64, NullDescriptorSet>,
    buffer_contents: BTreeMap<u64, Vec<u8>>,
    buffer_leases: BTreeMap<u64, VramLease>,
    image_leases: BTreeMap<u64, VramLease>,
    allocator: NullVramAllocator,
}

impl NullDevice {
    /// Устройство с бюджетом по умолчанию (256 МиБ).
    #[must_use]
    pub fn new() -> Self {
        Self::with_budget(DEFAULT_VRAM_BUDGET)
    }

    /// Устройство с заданным бюджетом VRAM.
    #[must_use]
    pub fn with_budget(total: u64) -> Self {
        Self {
            caps: Capabilities::headless(),
            next_buffer: 0,
            next_image: 0,
            next_pipeline: 0,
            next_swapchain: 0,
            next_query_pool: 0,
            next_descriptor_set: 0,
            buffers: BTreeMap::new(),
            images: BTreeMap::new(),
            pipelines: BTreeMap::new(),
            query_pools: BTreeMap::new(),
            descriptor_sets: BTreeMap::new(),
            buffer_contents: BTreeMap::new(),
            buffer_leases: BTreeMap::new(),
            image_leases: BTreeMap::new(),
            allocator: NullVramAllocator::new(total),
        }
    }

    /// Устройство с явными capabilities и бюджетом по умолчанию.
    #[must_use]
    pub fn with_capabilities(caps: Capabilities) -> Self {
        Self {
            caps,
            ..Self::with_budget(DEFAULT_VRAM_BUDGET)
        }
    }

    /// Число живых буферов.
    #[must_use]
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Число живых изображений.
    #[must_use]
    pub fn image_count(&self) -> usize {
        self.images.len()
    }

    /// Число живых pipeline.
    #[must_use]
    pub fn pipeline_count(&self) -> usize {
        self.pipelines.len()
    }

    /// Число живых дескрипторных наборов.
    #[must_use]
    pub fn descriptor_set_count(&self) -> usize {
        self.descriptor_sets.len()
    }

    /// Записанный дескриптор набора.
    #[must_use]
    pub fn descriptor_write(
        &self,
        id: DescriptorSetId,
        binding: u32,
        array_index: u32,
    ) -> Option<DescriptorResource> {
        self.descriptor_sets
            .get(&id.0)
            .and_then(|set| set.writes.get(&(binding, array_index)))
            .copied()
    }

    /// Занято VRAM.
    #[must_use]
    pub fn allocated_bytes(&self) -> u64 {
        self.allocator.used()
    }

    /// Снимок бюджета.
    #[must_use]
    pub fn budget(&self) -> BudgetSnapshot {
        self.allocator.budget()
    }

    fn image_bytes(desc: &GpuImageDesc) -> u64 {
        u64::from(desc.width)
            .saturating_mul(u64::from(desc.height))
            .saturating_mul(u64::from(desc.format.bytes_per_pixel()))
            .saturating_mul(u64::from(desc.mip_levels))
    }
}

impl Default for NullDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl RhiDevice for NullDevice {
    fn backend(&self) -> BackendKind {
        BackendKind::Null
    }

    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }

    fn create_buffer(&mut self, desc: &GpuBufferDesc) -> Result<BufferId, RhiError> {
        if desc.size == 0 {
            return Err(RhiError::validation("buffer size must be non-zero"));
        }
        if !desc.usage.any() {
            return Err(RhiError::validation("buffer usage must not be empty"));
        }
        let lease = self.allocator.allocate(desc.size, MemoryTag::Rendering)?;
        let id = self.next_buffer;
        self.next_buffer = self.next_buffer.wrapping_add(1);
        self.buffers.insert(id, *desc);
        self.buffer_leases.insert(id, lease);
        Ok(BufferId(id))
    }

    fn destroy_buffer(&mut self, id: BufferId) -> bool {
        let Some(_desc) = self.buffers.remove(&id.0) else {
            return false;
        };
        if let Some(lease) = self.buffer_leases.remove(&id.0) {
            self.allocator.free(lease);
        }
        true
    }

    fn create_image(&mut self, desc: &GpuImageDesc) -> Result<ImageId, RhiError> {
        if desc.width == 0 || desc.height == 0 {
            return Err(RhiError::validation("image dimensions must be non-zero"));
        }
        if desc.mip_levels == 0 {
            return Err(RhiError::validation("mip levels must be non-zero"));
        }
        if desc.width > self.caps.max_texture_dimension_2d
            || desc.height > self.caps.max_texture_dimension_2d
        {
            return Err(RhiError::validation("image exceeds max texture dimension"));
        }
        if !desc.usage.any() {
            return Err(RhiError::validation("image usage must not be empty"));
        }
        let bytes = Self::image_bytes(desc);
        let lease = self.allocator.allocate(bytes, MemoryTag::Rendering)?;
        let id = self.next_image;
        self.next_image = self.next_image.wrapping_add(1);
        self.images.insert(id, *desc);
        self.image_leases.insert(id, lease);
        Ok(ImageId(id))
    }

    fn destroy_image(&mut self, id: ImageId) -> bool {
        let Some(_desc) = self.images.remove(&id.0) else {
            return false;
        };
        if let Some(lease) = self.image_leases.remove(&id.0) {
            self.allocator.free(lease);
        }
        true
    }

    fn create_query_pool(&mut self, desc: &QueryPoolDesc) -> Result<QueryPoolId, RhiError> {
        if desc.count == 0 || desc.count > 4_096 {
            return Err(RhiError::validation("query pool count must be 1..=4096"));
        }
        let id = self.next_query_pool;
        self.next_query_pool = self.next_query_pool.wrapping_add(1);
        self.query_pools.insert(id, desc.count);
        Ok(QueryPoolId(id))
    }

    fn destroy_query_pool(&mut self, id: QueryPoolId) -> bool {
        self.query_pools.remove(&id.0).is_some()
    }

    fn read_query_pool(&mut self, id: QueryPoolId) -> Result<Vec<u64>, RhiError> {
        let count = *self
            .query_pools
            .get(&id.0)
            .ok_or_else(|| RhiError::validation("unknown query pool"))?;
        Ok((0..count).map(|index| u64::from(index) * 1_000).collect())
    }

    fn create_pipeline(&mut self, desc: &PipelineDesc) -> Result<PipelineId, RhiError> {
        let id = self.next_pipeline;
        self.next_pipeline = self.next_pipeline.wrapping_add(1);
        self.pipelines.insert(id, *desc);
        Ok(PipelineId(id))
    }

    fn destroy_pipeline(&mut self, id: PipelineId) -> bool {
        self.pipelines.remove(&id.0).is_some()
    }

    fn create_descriptor_set(
        &mut self,
        desc: &DescriptorSetDesc,
    ) -> Result<DescriptorSetId, RhiError> {
        if desc.bindings.is_empty() {
            return Err(RhiError::validation("descriptor set has no bindings"));
        }
        let mut seen = BTreeSet::new();
        for binding in &desc.bindings {
            if binding.count == 0 {
                return Err(RhiError::validation(
                    "descriptor binding count must be non-zero",
                ));
            }
            if !seen.insert(binding.binding) {
                return Err(RhiError::validation("duplicate descriptor binding"));
            }
        }
        let id = self.next_descriptor_set;
        self.next_descriptor_set = self.next_descriptor_set.wrapping_add(1);
        self.descriptor_sets.insert(
            id,
            NullDescriptorSet {
                bindings: desc.bindings.clone(),
                writes: BTreeMap::new(),
            },
        );
        Ok(DescriptorSetId(id))
    }

    fn update_descriptor_set(
        &mut self,
        id: DescriptorSetId,
        writes: &[DescriptorWrite],
    ) -> Result<(), RhiError> {
        {
            let set = self
                .descriptor_sets
                .get(&id.0)
                .ok_or_else(|| RhiError::validation("unknown descriptor set"))?;
            for write in writes {
                let binding = set
                    .bindings
                    .iter()
                    .find(|binding| binding.binding == write.binding)
                    .ok_or_else(|| RhiError::validation("unknown descriptor binding"))?;
                if write.array_index >= binding.count {
                    return Err(RhiError::validation(
                        "descriptor array index is out of bounds",
                    ));
                }
                let known = match (binding.kind, write.resource) {
                    (
                        BindingKind::UniformBuffer | BindingKind::StorageBuffer,
                        DescriptorResource::Buffer(buffer),
                    ) => self.buffers.contains_key(&buffer.0),
                    (
                        BindingKind::SampledImage | BindingKind::StorageImage,
                        DescriptorResource::Image(image),
                    ) => self.images.contains_key(&image.0),
                    _ => return Err(RhiError::validation("descriptor resource type mismatch")),
                };
                if !known {
                    return Err(RhiError::validation("unknown descriptor resource"));
                }
            }
        }
        let Some(set) = self.descriptor_sets.get_mut(&id.0) else {
            return Err(RhiError::validation("unknown descriptor set"));
        };
        for write in writes {
            set.writes
                .insert((write.binding, write.array_index), write.resource);
        }
        Ok(())
    }

    fn destroy_descriptor_set(&mut self, id: DescriptorSetId) -> bool {
        self.descriptor_sets.remove(&id.0).is_some()
    }

    fn write_buffer(&mut self, id: BufferId, offset: u64, data: &[u8]) -> Result<(), RhiError> {
        let desc = *self
            .buffers
            .get(&id.0)
            .ok_or_else(|| RhiError::validation("unknown buffer"))?;
        if !is_host_visible(desc.usage) {
            return Err(RhiError::validation("buffer is not host-visible"));
        }
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| RhiError::validation("buffer write overflows"))?;
        if end > desc.size {
            return Err(RhiError::validation("buffer write is out of bounds"));
        }
        let size = usize::try_from(desc.size)
            .map_err(|_| RhiError::validation("buffer size exceeds address space"))?;
        let contents = self
            .buffer_contents
            .entry(id.0)
            .or_insert_with(|| vec![0; size]);
        contents[offset as usize..end as usize].copy_from_slice(data);
        Ok(())
    }

    fn read_buffer(&mut self, id: BufferId) -> Result<Vec<u8>, RhiError> {
        let desc = *self
            .buffers
            .get(&id.0)
            .ok_or_else(|| RhiError::validation("unknown buffer"))?;
        if !is_host_visible(desc.usage) {
            return Err(RhiError::validation("buffer is not host-visible"));
        }
        if let Some(contents) = self.buffer_contents.get(&id.0) {
            return Ok(contents.clone());
        }
        let size = usize::try_from(desc.size)
            .map_err(|_| RhiError::validation("buffer size exceeds address space"))?;
        Ok(vec![0; size])
    }

    fn create_command_buffer(&mut self) -> Result<Box<dyn CommandBuffer>, RhiError> {
        let descriptor_sets = self
            .descriptor_sets
            .iter()
            .map(|(id, set)| (*id, set.bindings.clone()))
            .collect();
        Ok(Box::new(NullCommandBuffer::with_resources(
            self.query_pools.clone(),
            self.buffers
                .iter()
                .map(|(id, desc)| (*id, desc.usage))
                .collect(),
            self.images.keys().copied().collect(),
            descriptor_sets,
        )))
    }

    fn create_swapchain(&mut self, desc: &SwapchainDesc) -> Result<Box<dyn Swapchain>, RhiError> {
        if desc.width == 0 || desc.height == 0 {
            return Err(RhiError::validation("swapchain size must be non-zero"));
        }
        if !self.caps.supports_color_format(desc.format) {
            return Err(RhiError::validation("unsupported swapchain format"));
        }
        if !self.caps.supports_present_mode(desc.present_mode) {
            return Err(RhiError::validation("unsupported present mode"));
        }
        if desc.frames_in_flight == 0 || desc.frames_in_flight > self.caps.max_frames_in_flight {
            return Err(RhiError::validation("invalid frames_in_flight"));
        }
        let id = SwapchainId(self.next_swapchain);
        self.next_swapchain = self.next_swapchain.wrapping_add(1);
        Ok(Box::new(NullSwapchain::new(id, *desc)))
    }

    fn wait_idle(&mut self) -> Result<(), RhiError> {
        Ok(())
    }
}

/// Null-swapchain: циклически выдаёт индексы кадров.
pub struct NullSwapchain {
    id: SwapchainId,
    width: u32,
    height: u32,
    format: TextureFormat,
    present_mode: PresentMode,
    frames_in_flight: u32,
    acquired: Option<u32>,
    frame: u64,
}

impl NullSwapchain {
    fn new(id: SwapchainId, desc: SwapchainDesc) -> Self {
        Self {
            id,
            width: desc.width,
            height: desc.height,
            format: desc.format,
            present_mode: desc.present_mode,
            frames_in_flight: desc.frames_in_flight,
            acquired: None,
            frame: 0,
        }
    }
}

impl Swapchain for NullSwapchain {
    fn id(&self) -> SwapchainId {
        self.id
    }

    fn format(&self) -> TextureFormat {
        self.format
    }

    fn present_mode(&self) -> PresentMode {
        self.present_mode
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), RhiError> {
        if width == 0 || height == 0 {
            return Err(RhiError::validation("swapchain size must be non-zero"));
        }
        self.width = width;
        self.height = height;
        self.acquired = None;
        Ok(())
    }

    fn acquire(&mut self) -> Result<u32, RhiError> {
        if self.acquired.is_some() {
            return Err(RhiError::validation("swapchain image already acquired"));
        }
        let index = (self.frame % u64::from(self.frames_in_flight)) as u32;
        self.frame = self.frame.wrapping_add(1);
        self.acquired = Some(index);
        Ok(index)
    }

    fn present(&mut self, image_index: u32) -> Result<(), RhiError> {
        match self.acquired {
            Some(current) if current == image_index => {
                self.acquired = None;
                Ok(())
            }
            Some(_) => Err(RhiError::validation("presented image index mismatch")),
            None => Err(RhiError::validation("present without acquire")),
        }
    }
}

/// Null-командный буфер: проверяет порядок вызовов и считает команды.
#[derive(Clone, Debug, Default)]
pub struct NullCommandBuffer {
    begun: bool,
    in_pass: bool,
    draws: u32,
    dispatches: u32,
    copies: u32,
    barriers: usize,
    passes: u32,
    pipeline: Option<PipelineId>,
    bound_set: Option<DescriptorSetId>,
    timestamps: Vec<(QueryPoolId, u32)>,
    query_pools: BTreeMap<u64, u32>,
    buffers: BTreeMap<u64, BufferUsage>,
    images: BTreeSet<u64>,
    descriptor_sets: BTreeMap<u64, Vec<DescriptorBinding>>,
}

impl NullCommandBuffer {
    /// Новый пустой буфер.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Буфер, знающий ёмкости пулов запросов устройства.
    #[must_use]
    pub fn with_query_pools(query_pools: BTreeMap<u64, u32>) -> Self {
        Self {
            query_pools,
            ..Self::default()
        }
    }

    /// Буфер, знающий ресурсы устройства (для валидации копий и наборов).
    #[must_use]
    pub fn with_resources(
        query_pools: BTreeMap<u64, u32>,
        buffers: BTreeMap<u64, BufferUsage>,
        images: BTreeSet<u64>,
        descriptor_sets: BTreeMap<u64, Vec<DescriptorBinding>>,
    ) -> Self {
        Self {
            query_pools,
            buffers,
            images,
            descriptor_sets,
            ..Self::default()
        }
    }

    /// Число draw-вызовов.
    #[must_use]
    pub fn draws(&self) -> u32 {
        self.draws
    }

    /// Число render pass.
    #[must_use]
    pub fn passes(&self) -> u32 {
        self.passes
    }

    /// Число применённых барьеров.
    #[must_use]
    pub fn barriers_applied(&self) -> usize {
        self.barriers
    }

    /// Число копирующих команд.
    #[must_use]
    pub fn copies(&self) -> u32 {
        self.copies
    }

    /// Привязанный дескрипторный набор.
    #[must_use]
    pub fn bound_descriptor_set(&self) -> Option<DescriptorSetId> {
        self.bound_set
    }

    /// Установленный pipeline.
    #[must_use]
    pub fn pipeline(&self) -> Option<PipelineId> {
        self.pipeline
    }

    /// Записанные таймстемпы.
    #[must_use]
    pub fn timestamps(&self) -> &[(QueryPoolId, u32)] {
        &self.timestamps
    }
}

impl CommandBuffer for NullCommandBuffer {
    fn begin(&mut self) -> Result<(), RhiError> {
        if self.begun {
            return Err(RhiError::validation("command buffer already begun"));
        }
        self.begun = true;
        Ok(())
    }

    fn end(&mut self) -> Result<(), RhiError> {
        if !self.begun {
            return Err(RhiError::validation("command buffer is not begun"));
        }
        if self.in_pass {
            return Err(RhiError::validation("render pass is still open"));
        }
        self.begun = false;
        Ok(())
    }

    fn begin_render_pass(&mut self, desc: &RenderPassDesc) -> Result<(), RhiError> {
        if !self.begun {
            return Err(RhiError::validation("command buffer is not begun"));
        }
        if self.in_pass {
            return Err(RhiError::validation("render pass already open"));
        }
        if !desc.has_attachments() {
            return Err(RhiError::validation("render pass has no attachments"));
        }
        self.in_pass = true;
        self.passes = self.passes.saturating_add(1);
        Ok(())
    }

    fn end_render_pass(&mut self) -> Result<(), RhiError> {
        if !self.in_pass {
            return Err(RhiError::validation("no open render pass"));
        }
        self.in_pass = false;
        Ok(())
    }

    fn reset_query_pool(
        &mut self,
        pool: QueryPoolId,
        first: u32,
        count: u32,
    ) -> Result<(), RhiError> {
        if !self.begun {
            return Err(RhiError::validation("command buffer is not begun"));
        }
        let capacity = *self
            .query_pools
            .get(&pool.0)
            .ok_or_else(|| RhiError::validation("unknown query pool"))?;
        let in_bounds = match first.checked_add(count) {
            Some(end) => count > 0 && end <= capacity,
            None => false,
        };
        if !in_bounds {
            return Err(RhiError::validation("query range is out of bounds"));
        }
        Ok(())
    }

    fn write_timestamp(&mut self, pool: QueryPoolId, index: u32) -> Result<(), RhiError> {
        if !self.begun {
            return Err(RhiError::validation("command buffer is not begun"));
        }
        let capacity = *self
            .query_pools
            .get(&pool.0)
            .ok_or_else(|| RhiError::validation("unknown query pool"))?;
        if index >= capacity {
            return Err(RhiError::validation("timestamp index is out of bounds"));
        }
        self.timestamps.push((pool, index));
        Ok(())
    }

    fn set_pipeline(&mut self, pipeline: PipelineId) -> Result<(), RhiError> {
        if !self.begun {
            return Err(RhiError::validation("command buffer is not begun"));
        }
        self.pipeline = Some(pipeline);
        Ok(())
    }

    fn bind_descriptor_set(&mut self, set: DescriptorSetId) -> Result<(), RhiError> {
        if !self.begun {
            return Err(RhiError::validation("command buffer is not begun"));
        }
        if self.pipeline.is_none() {
            return Err(RhiError::validation(
                "descriptor set bound without a pipeline",
            ));
        }
        if !self.descriptor_sets.contains_key(&set.0) {
            return Err(RhiError::validation("unknown descriptor set"));
        }
        self.bound_set = Some(set);
        Ok(())
    }

    fn copy_buffer_to_image(&mut self, buffer: BufferId, image: ImageId) -> Result<(), RhiError> {
        if !self.begun {
            return Err(RhiError::validation("command buffer is not begun"));
        }
        let Some(usage) = self.buffers.get(&buffer.0) else {
            return Err(RhiError::validation("unknown source buffer"));
        };
        if !is_host_visible(*usage) {
            return Err(RhiError::validation("copy source must be a staging buffer"));
        }
        if !self.images.contains(&image.0) {
            return Err(RhiError::validation("unknown destination image"));
        }
        self.copies = self.copies.saturating_add(1);
        Ok(())
    }

    fn copy_image_to_buffer(&mut self, image: ImageId, buffer: BufferId) -> Result<(), RhiError> {
        if !self.begun {
            return Err(RhiError::validation("command buffer is not begun"));
        }
        if !self.images.contains(&image.0) {
            return Err(RhiError::validation("unknown source image"));
        }
        let Some(usage) = self.buffers.get(&buffer.0) else {
            return Err(RhiError::validation("unknown destination buffer"));
        };
        if !is_host_visible(*usage) {
            return Err(RhiError::validation(
                "copy destination must be a staging buffer",
            ));
        }
        self.copies = self.copies.saturating_add(1);
        Ok(())
    }

    fn draw(&mut self, _vertices: u32, _instances: u32) -> Result<(), RhiError> {
        if !self.in_pass {
            return Err(RhiError::validation("draw outside render pass"));
        }
        self.draws = self.draws.saturating_add(1);
        Ok(())
    }

    fn dispatch(&mut self, _groups: [u32; 3]) -> Result<(), RhiError> {
        if !self.begun || self.in_pass {
            return Err(RhiError::validation("dispatch outside compute scope"));
        }
        self.dispatches = self.dispatches.saturating_add(1);
        Ok(())
    }

    fn barriers(&mut self, batch: &BarrierBatch) -> Result<(), RhiError> {
        if !self.begun {
            return Err(RhiError::validation("command buffer is not begun"));
        }
        self.barriers = self.barriers.saturating_add(batch.len());
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<(), RhiError> {
        if self.begun || self.in_pass {
            return Err(RhiError::validation("command buffer is still recording"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rhi::cmd::ImageState;
    use crate::rhi::types::BufferUsage;
    use std::collections::BTreeSet;

    fn buffer(size: u64) -> GpuBufferDesc {
        GpuBufferDesc::new("test", size, BufferUsage::vertex())
    }

    #[test]
    fn null_device_tracks_resources_and_vram() {
        let mut device = NullDevice::with_budget(1024);
        let id = device.create_buffer(&buffer(256)).unwrap();
        assert_eq!(device.buffer_count(), 1);
        assert_eq!(device.allocated_bytes(), 256);
        assert!(device.destroy_buffer(id));
        assert_eq!(device.buffer_count(), 0);
        assert_eq!(device.allocated_bytes(), 0);
        assert!(!device.destroy_buffer(id));
    }

    #[test]
    fn null_device_rejects_out_of_memory() {
        let mut device = NullDevice::with_budget(128);
        let error = device.create_buffer(&buffer(256)).unwrap_err();
        assert_eq!(error, RhiError::OutOfMemory { requested: 256 });
    }

    #[test]
    fn null_device_validates_images() {
        let mut device = NullDevice::with_budget(4096);
        let mut desc = GpuImageDesc::new("tex", 0, 16, TextureFormat::Rgba8Srgb);
        assert!(device.create_image(&desc).is_err());
        desc.width = 16;
        desc.mip_levels = 0;
        assert!(device.create_image(&desc).is_err());
        desc.mip_levels = 1;
        let image = device.create_image(&desc).unwrap();
        assert_eq!(device.allocated_bytes(), 16 * 16 * 4);
        assert!(device.destroy_image(image));
    }

    #[test]
    fn null_swapchain_acquires_presents_and_resizes() {
        let mut device = NullDevice::new();
        let mut swapchain = device
            .create_swapchain(&SwapchainDesc::new("main", 1280, 720))
            .unwrap();
        assert_eq!(swapchain.size(), (1280, 720));
        let first = swapchain.acquire().unwrap();
        assert!(swapchain.acquire().is_err());
        swapchain.present(first).unwrap();
        let second = swapchain.acquire().unwrap();
        assert!(swapchain.present(second + 1).is_err());
        swapchain.present(second).unwrap();
        swapchain.resize(800, 600).unwrap();
        assert_eq!(swapchain.size(), (800, 600));
    }

    #[test]
    fn null_command_buffer_records_draws_and_barriers() {
        let mut cmd = NullCommandBuffer::new();
        cmd.begin().unwrap();
        let pass = RenderPassDesc::new("main").with_color(ImageId(1));
        cmd.begin_render_pass(&pass).unwrap();
        cmd.set_pipeline(PipelineId(0)).unwrap();
        cmd.draw(3, 1).unwrap();
        cmd.end_render_pass().unwrap();
        let mut barriers = BarrierBatch::new();
        barriers.transition(ImageId(1), ImageState::ColorAttachment, ImageState::Present);
        cmd.barriers(&barriers).unwrap();
        cmd.end().unwrap();
        assert_eq!(cmd.draws(), 1);
        assert_eq!(cmd.passes(), 1);
        assert_eq!(cmd.barriers_applied(), 1);
        Box::new(cmd).finish().unwrap();
    }

    #[test]
    fn null_command_buffer_rejects_wrong_order() {
        let mut cmd = NullCommandBuffer::new();
        assert!(cmd.end().is_err());
        cmd.begin().unwrap();
        assert!(cmd.begin().is_err());
        assert!(cmd
            .begin_render_pass(&RenderPassDesc::new("empty"))
            .is_err());
        cmd.end().unwrap();
    }

    #[test]
    fn null_ids_are_deterministic() {
        let mut first = NullDevice::new();
        let mut second = NullDevice::new();
        assert_eq!(
            first.create_buffer(&buffer(16)).unwrap(),
            second.create_buffer(&buffer(16)).unwrap()
        );
    }

    #[test]
    fn null_query_pools_are_deterministic() {
        let mut device = NullDevice::new();
        assert!(device
            .create_query_pool(&QueryPoolDesc::new("empty", 0))
            .is_err());
        let pool = device
            .create_query_pool(&QueryPoolDesc::new("zones", 2))
            .unwrap();
        assert_eq!(device.read_query_pool(pool).unwrap(), vec![0, 1_000]);

        let mut commands = NullCommandBuffer::with_query_pools(BTreeMap::from([(pool.0, 2)]));
        commands.begin().unwrap();
        commands.reset_query_pool(pool, 0, 2).unwrap();
        commands.write_timestamp(pool, 0).unwrap();
        commands.write_timestamp(pool, 1).unwrap();
        assert_eq!(commands.timestamps(), &[(pool, 0), (pool, 1)]);
        assert!(commands.write_timestamp(pool, 2).is_err());
        assert!(commands.reset_query_pool(pool, 1, 2).is_err());
        commands.end().unwrap();
        Box::new(commands).finish().unwrap();

        assert!(device.destroy_query_pool(pool));
        assert!(!device.destroy_query_pool(pool));
        assert!(device.read_query_pool(pool).is_err());
    }

    #[test]
    fn create_device_reports_backend_status() {
        assert!(crate::create_device(BackendKind::Null).is_ok());
        assert!(matches!(
            crate::create_device(BackendKind::Dx12),
            Err(RhiError::Unsupported { .. })
        ));
        #[cfg(not(feature = "vulkan"))]
        assert!(matches!(
            crate::create_device(BackendKind::Vulkan),
            Err(RhiError::Unsupported { .. })
        ));
        #[cfg(feature = "vulkan")]
        {
            // С включённой feature исход зависит от машины: устройство или
            // типизированная ошибка (нет loader-а/GPU), но не паника.
            let _ = crate::create_device(BackendKind::Vulkan);
        }
    }

    #[test]
    fn null_buffer_roundtrip_and_host_visibility() {
        let mut device = NullDevice::new();
        let staging = device
            .create_buffer(&GpuBufferDesc::new("staging", 16, BufferUsage::staging()))
            .unwrap();
        device.write_buffer(staging, 4, &[1, 2, 3, 4]).unwrap();
        let bytes = device.read_buffer(staging).unwrap();
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[4..8], &[1, 2, 3, 4]);
        assert_eq!(&bytes[0..4], &[0, 0, 0, 0]);
        assert!(device.write_buffer(staging, 14, &[1, 2, 3, 4]).is_err());

        let vertex = device.create_buffer(&buffer(16)).unwrap();
        assert!(device.write_buffer(vertex, 0, &[0]).is_err());
        assert!(device.read_buffer(vertex).is_err());
    }

    #[test]
    fn null_descriptor_sets_validate_layout_and_resources() {
        let mut device = NullDevice::new();
        let staging = device
            .create_buffer(&GpuBufferDesc::new("staging", 64, BufferUsage::staging()))
            .unwrap();
        let storage = device
            .create_buffer(&GpuBufferDesc::new("storage", 64, BufferUsage::storage()))
            .unwrap();
        let image = device
            .create_image(&GpuImageDesc::new("tex", 4, 4, TextureFormat::Rgba8Srgb))
            .unwrap();

        let layout = [
            DescriptorBinding::new(0, BindingKind::StorageBuffer),
            DescriptorBinding::new(1, BindingKind::SampledImage).with_count(4),
        ];
        assert!(device
            .create_descriptor_set(&DescriptorSetDesc::new("empty"))
            .is_err());
        let set = device
            .create_descriptor_set(&DescriptorSetDesc::new("bindless").with_bindings(&layout))
            .unwrap();
        assert_eq!(device.descriptor_set_count(), 1);

        assert!(device
            .update_descriptor_set(set, &[DescriptorWrite::image(0, 0, image)])
            .is_err());
        assert!(device
            .update_descriptor_set(set, &[DescriptorWrite::image(1, 4, image)])
            .is_err());
        assert!(device
            .update_descriptor_set(set, &[DescriptorWrite::buffer(9, 0, storage)])
            .is_err());

        device
            .update_descriptor_set(
                set,
                &[
                    DescriptorWrite::buffer(0, 0, storage),
                    DescriptorWrite::image(1, 3, image),
                ],
            )
            .unwrap();
        assert_eq!(
            device.descriptor_write(set, 0, 0),
            Some(DescriptorResource::Buffer(storage))
        );
        assert_eq!(
            device.descriptor_write(set, 1, 3),
            Some(DescriptorResource::Image(image))
        );

        let mut commands = NullCommandBuffer::with_resources(
            BTreeMap::new(),
            BTreeMap::from([
                (staging.0, BufferUsage::staging()),
                (storage.0, BufferUsage::storage()),
            ]),
            BTreeSet::from([image.0]),
            BTreeMap::from([(set.0, layout.to_vec())]),
        );
        commands.begin().unwrap();
        assert!(commands.bind_descriptor_set(set).is_err());
        commands.set_pipeline(PipelineId(0)).unwrap();
        commands.bind_descriptor_set(set).unwrap();
        commands.copy_buffer_to_image(staging, image).unwrap();
        commands.copy_image_to_buffer(image, staging).unwrap();
        assert_eq!(commands.copies(), 2);
        assert_eq!(commands.bound_descriptor_set(), Some(set));
        assert!(commands.copy_buffer_to_image(BufferId(99), image).is_err());
        // Копии обязаны использовать staging-буфер (ADR-0039).
        assert!(commands.copy_buffer_to_image(storage, image).is_err());
        assert!(commands.copy_image_to_buffer(image, storage).is_err());
        commands.end().unwrap();
        Box::new(commands).finish().unwrap();

        assert!(device.destroy_descriptor_set(set));
        assert!(!device.destroy_descriptor_set(set));
        assert_eq!(device.descriptor_set_count(), 0);
    }

    #[test]
    fn vram_pressure_levels() {
        let mut allocator = NullVramAllocator::new(100);
        allocator.allocate(50, MemoryTag::Rendering).unwrap();
        assert_eq!(allocator.budget().pressure, Pressure::Normal);
        allocator.allocate(40, MemoryTag::Rendering).unwrap();
        assert_eq!(allocator.budget().pressure, Pressure::High);
        allocator.allocate(9, MemoryTag::Rendering).unwrap();
        assert_eq!(allocator.budget().pressure, Pressure::Critical);
    }
}
