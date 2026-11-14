//! Контракт RHI-устройства.

use super::{
    cmd::CommandBuffer,
    error::RhiError,
    swapchain::Swapchain,
    types::{
        BackendKind, BufferId, Capabilities, DescriptorSetDesc, DescriptorSetId, DescriptorWrite,
        DeviceSignature, GpuBufferDesc, GpuImageDesc, ImageId, PipelineDesc, PipelineId,
        QueryPoolDesc, QueryPoolId, SamplerDesc, SamplerId, SwapchainDesc,
    },
};

/// GPU-устройство: владеет ресурсами и создаёт объекты RHI.
///
/// Устройство живёт дольше рендереров (ADR «Контракт Renderer и владение»);
/// `flux-engine` создаёт его на старте, рендереры получают `&mut dyn RhiDevice`.
pub trait RhiDevice: Send {
    /// Бэкенд устройства.
    fn backend(&self) -> BackendKind;

    /// Возможности устройства.
    fn capabilities(&self) -> &Capabilities;

    /// Подпись устройства для инвалидации дисковых кэшей (PSO).
    fn device_signature(&self) -> DeviceSignature;

    /// Сериализует текущий кэш PSO (блоб `VkPipelineCache`).
    ///
    /// Пустой вектор — драйвер ещё не накопил данные или бэкенд не кэширует
    /// PSO. Блоб сохраняется на диск вместе с подписью устройства
    /// (см. `PipelineCacheFile`).
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — драйвер не отдал данные кэша.
    fn pipeline_cache(&self) -> Result<Vec<u8>, RhiError>;

    /// Загружает блоб кэша PSO, полученный из [`RhiDevice::pipeline_cache`].
    ///
    /// Пустой блоб — no-op. Загрузка заменяет текущий кэш устройства.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — драйвер отверг данные кэша.
    fn load_pipeline_cache(&mut self, data: &[u8]) -> Result<(), RhiError>;

    /// Создаёт GPU-буфер.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — пустой/некорректный дескриптор;
    /// [`RhiError::OutOfMemory`] — не хватает VRAM.
    fn create_buffer(&mut self, desc: &GpuBufferDesc) -> Result<BufferId, RhiError>;

    /// Уничтожает буфер. `false` — хэндл неизвестен/уже уничтожен.
    fn destroy_buffer(&mut self, id: BufferId) -> bool;

    /// Создаёт GPU-изображение.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — нулевой размер, превышен лимит или mips;
    /// [`RhiError::OutOfMemory`] — не хватает VRAM.
    fn create_image(&mut self, desc: &GpuImageDesc) -> Result<ImageId, RhiError>;

    /// Уничтожает изображение.
    fn destroy_image(&mut self, id: ImageId) -> bool;

    /// Создаёт сэмплер (ADR-0041).
    ///
    /// Сэмплеры иммутабельны: изменение фильтрации — это новый сэмплер.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — некорректные границы полей или анизотропия
    /// не поддержана устройством.
    fn create_sampler(&mut self, desc: &SamplerDesc) -> Result<SamplerId, RhiError>;

    /// Уничтожает сэмплер. `false` — хэндл неизвестен/уже уничтожен.
    fn destroy_sampler(&mut self, id: SamplerId) -> bool;

    /// Создаёт pipeline state object.
    fn create_pipeline(&mut self, desc: &PipelineDesc) -> Result<PipelineId, RhiError>;

    /// Уничтожает pipeline.
    fn destroy_pipeline(&mut self, id: PipelineId) -> bool;

    /// Создаёт дескрипторный набор по layout-у (set 0).
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — пустой layout, повтор/нулевой размер биндинга.
    fn create_descriptor_set(
        &mut self,
        desc: &DescriptorSetDesc,
    ) -> Result<DescriptorSetId, RhiError>;

    /// Обновляет дескрипторы набора.
    ///
    /// Набор не должен быть в полёте: дескрипторы живут в кольце кадров
    /// `frames_in_flight` (ADR-0039), обновление — на границе acquire.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — неизвестный набор/биндинг, индекс вне массива,
    /// несовпадение типа ресурса или неизвестный ресурс.
    fn update_descriptor_set(
        &mut self,
        id: DescriptorSetId,
        writes: &[DescriptorWrite],
    ) -> Result<(), RhiError>;

    /// Уничтожает дескрипторный набор.
    fn destroy_descriptor_set(&mut self, id: DescriptorSetId) -> bool;

    /// Записывает данные в host-visible буфер (staging/upload).
    ///
    /// Staging-буферы живут в кольце кадров `frames_in_flight` (ADR-0039);
    /// копирование в device-local выполняет copy-узел Render Graph.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — буфер неизвестен, диапазон вне размера или
    /// память не host-visible.
    fn write_buffer(&mut self, id: BufferId, offset: u64, data: &[u8]) -> Result<(), RhiError>;

    /// Читает содержимое host-visible буфера (readback).
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — буфер неизвестен или память не host-visible.
    fn read_buffer(&mut self, id: BufferId) -> Result<Vec<u8>, RhiError>;

    /// Создаёт пул GPU-запросов (таймстемпы).
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — нулевое/слишком большое число запросов.
    fn create_query_pool(&mut self, desc: &QueryPoolDesc) -> Result<QueryPoolId, RhiError>;

    /// Уничтожает пул запросов.
    fn destroy_query_pool(&mut self, id: QueryPoolId) -> bool;

    /// Читает результаты пула после завершения GPU-работы.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — пул неизвестен или чтение не удалось.
    fn read_query_pool(&mut self, id: QueryPoolId) -> Result<Vec<u64>, RhiError>;

    /// Создаёт командный буфер.
    fn create_command_buffer(&mut self) -> Result<Box<dyn CommandBuffer>, RhiError>;

    /// Создаёт swapchain для окна/таргета.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — размер/формат/режим не поддержан;
    /// [`RhiError::Surface`] — поверхность недоступна.
    fn create_swapchain(&mut self, desc: &SwapchainDesc) -> Result<Box<dyn Swapchain>, RhiError>;

    /// Ждёт завершения всех GPU-работ.
    fn wait_idle(&mut self) -> Result<(), RhiError>;
}
