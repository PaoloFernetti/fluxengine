//! TransferSystem: staging-кольцо, батчи загрузок и fence-ретирмент.
//!
//! Итерация I2 — слой планирования: загрузка получает VRAM-лизинг и место
//! в staging-кольце, `submit_batch` фиксирует батч на кадре, `retire`
//! освобождает лизинги после fence-а. Реальные `vkCmdCopyBufferToImage`
//! добавятся, когда RHI получит copy-команды (I3/I5); контракт батча при
//! этом не меняется.

use super::super::rhi::{ImageId, RhiError, VramAllocator, VramLease};
use flux_core::memory::MemoryTag;

/// Staging-кольцо по умолчанию: 64 МиБ (диапазон 64–128 МиБ).
pub const DEFAULT_STAGING_BYTES: u64 = 64 * 1024 * 1024;

/// Загрузка, ожидающая отправки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UploadJob {
    /// Детерминированный id загрузки.
    pub id: u64,
    /// Целевое изображение.
    pub image: ImageId,
    /// Размер, байт.
    pub bytes: u64,
    /// VRAM-лизинг под загрузку.
    pub lease: VramLease,
}

/// Отправленный батч, ждущий fence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InFlightBatch {
    /// Детерминированный id батча.
    pub id: u64,
    /// Кадр, на котором батч отправлен.
    pub frame: u64,
    /// Суммарный размер, байт.
    pub bytes: u64,
    /// Загрузки батча.
    pub jobs: Vec<UploadJob>,
}

/// Планировщик загрузок ассет → GPU.
#[derive(Clone, Debug)]
pub struct TransferSystem {
    staging_capacity: u64,
    staging_used: u64,
    pending: Vec<UploadJob>,
    in_flight: Vec<InFlightBatch>,
    next_job: u64,
    next_batch: u64,
}

impl TransferSystem {
    /// Система с заданным размером staging-кольца.
    #[must_use]
    pub fn new(staging_capacity: u64) -> Self {
        Self {
            staging_capacity,
            staging_used: 0,
            pending: Vec::new(),
            in_flight: Vec::new(),
            next_job: 0,
            next_batch: 0,
        }
    }

    /// Система с кольцом по умолчанию (64 МиБ).
    #[must_use]
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_STAGING_BYTES)
    }

    /// Размер staging-кольца.
    #[must_use]
    pub fn staging_capacity(&self) -> u64 {
        self.staging_capacity
    }

    /// Занято staging (ожидающие + в полёте), байт.
    #[must_use]
    pub fn staging_used(&self) -> u64 {
        self.staging_used
    }

    /// Число ожидающих загрузок.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Размер ожидающих загрузок, байт.
    #[must_use]
    pub fn pending_bytes(&self) -> u64 {
        self.pending.iter().map(|job| job.bytes).sum()
    }

    /// Число батчей в полёте.
    #[must_use]
    pub fn in_flight_batches(&self) -> usize {
        self.in_flight.len()
    }

    /// Размер батчей в полёте, байт.
    #[must_use]
    pub fn in_flight_bytes(&self) -> u64 {
        self.in_flight.iter().map(|batch| batch.bytes).sum()
    }

    /// Нет ли ожидающих и летящих батчей.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.pending.is_empty() && self.in_flight.is_empty()
    }

    /// Ставит загрузку в очередь и берёт VRAM-лизинг.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — нулевой размер;
    /// [`RhiError::OutOfMemory`] — staging-кольцо или VRAM исчерпаны.
    pub fn upload(
        &mut self,
        allocator: &mut dyn VramAllocator,
        image: ImageId,
        bytes: u64,
        tag: MemoryTag,
    ) -> Result<u64, RhiError> {
        if bytes == 0 {
            return Err(RhiError::validation("zero-sized upload"));
        }
        let occupied = self.staging_used.saturating_add(bytes);
        if occupied > self.staging_capacity {
            return Err(RhiError::OutOfMemory { requested: bytes });
        }
        let lease = allocator.allocate(bytes, tag)?;
        self.staging_used = occupied;
        let id = self.next_job;
        self.next_job = self.next_job.wrapping_add(1);
        self.pending.push(UploadJob {
            id,
            image,
            bytes,
            lease,
        });
        Ok(id)
    }

    /// Фиксирует батч на кадре `frame` и возвращает его id.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — нет ожидающих загрузок.
    pub fn submit_batch(&mut self, frame: u64) -> Result<u64, RhiError> {
        if self.pending.is_empty() {
            return Err(RhiError::validation("empty upload batch"));
        }
        let bytes = self.pending.iter().map(|job| job.bytes).sum();
        let jobs = std::mem::take(&mut self.pending);
        let id = self.next_batch;
        self.next_batch = self.next_batch.wrapping_add(1);
        self.in_flight.push(InFlightBatch {
            id,
            frame,
            bytes,
            jobs,
        });
        Ok(id)
    }

    /// Освобождает батчи с `frame <= completed_frame`.
    ///
    /// Возвращает число завершённых батчей. Вызывается только между кадрами
    /// и после fence-а (ADR-0010/ADR-0013).
    pub fn retire(&mut self, allocator: &mut dyn VramAllocator, completed_frame: u64) -> usize {
        let mut retired = Vec::new();
        let mut kept = Vec::with_capacity(self.in_flight.len());
        for batch in std::mem::take(&mut self.in_flight) {
            if batch.frame <= completed_frame {
                retired.push(batch);
            } else {
                kept.push(batch);
            }
        }
        self.in_flight = kept;
        let mut completed = 0;
        for batch in retired {
            for job in &batch.jobs {
                let _ = allocator.free(job.lease);
            }
            self.staging_used = self.staging_used.saturating_sub(batch.bytes);
            completed += 1;
        }
        completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rhi::NullVramAllocator;

    #[test]
    fn upload_submit_and_retire_free_vram() {
        let mut allocator = NullVramAllocator::new(1 << 20);
        let mut transfer = TransferSystem::new(4096);
        let job = transfer
            .upload(&mut allocator, ImageId(1), 1024, MemoryTag::Rendering)
            .unwrap();
        assert_eq!(job, 0);
        assert_eq!(transfer.pending_count(), 1);
        assert_eq!(transfer.staging_used(), 1024);
        assert_eq!(allocator.budget().used, 1024);
        let batch = transfer.submit_batch(7).unwrap();
        assert_eq!(batch, 0);
        assert_eq!(transfer.pending_count(), 0);
        assert_eq!(transfer.in_flight_batches(), 1);
        assert_eq!(transfer.retire(&mut allocator, 6), 0);
        assert_eq!(transfer.retire(&mut allocator, 7), 1);
        assert_eq!(transfer.staging_used(), 0);
        assert_eq!(allocator.budget().used, 0);
        assert!(transfer.is_idle());
    }

    #[test]
    fn staging_overflow_and_empty_batch_are_typed_errors() {
        let mut allocator = NullVramAllocator::new(1 << 20);
        let mut transfer = TransferSystem::new(100);
        assert!(matches!(
            transfer.upload(&mut allocator, ImageId(1), 101, MemoryTag::Rendering),
            Err(RhiError::OutOfMemory { .. })
        ));
        assert!(matches!(
            transfer.submit_batch(1),
            Err(RhiError::Validation(_))
        ));
    }

    #[test]
    fn vram_exhaustion_is_reported() {
        let mut allocator = NullVramAllocator::new(100);
        let mut transfer = TransferSystem::new(1024);
        assert!(matches!(
            transfer.upload(&mut allocator, ImageId(1), 101, MemoryTag::Rendering),
            Err(RhiError::OutOfMemory { .. })
        ));
        assert_eq!(transfer.pending_count(), 0);
        assert_eq!(transfer.staging_used(), 0);
    }

    #[test]
    fn batches_and_ids_are_deterministic() {
        let mut first_allocator = NullVramAllocator::new(1 << 20);
        let mut second_allocator = NullVramAllocator::new(1 << 20);
        let mut first = TransferSystem::new(1024);
        let mut second = TransferSystem::new(1024);
        first
            .upload(&mut first_allocator, ImageId(1), 16, MemoryTag::Rendering)
            .unwrap();
        first
            .upload(&mut first_allocator, ImageId(2), 32, MemoryTag::Rendering)
            .unwrap();
        second
            .upload(&mut second_allocator, ImageId(1), 16, MemoryTag::Rendering)
            .unwrap();
        second
            .upload(&mut second_allocator, ImageId(2), 32, MemoryTag::Rendering)
            .unwrap();
        assert_eq!(
            first.submit_batch(3).unwrap(),
            second.submit_batch(3).unwrap()
        );
        assert_eq!(first.pending_bytes(), second.pending_bytes());
        assert_eq!(
            first.retire(&mut first_allocator, 3),
            second.retire(&mut second_allocator, 3)
        );
    }
}
