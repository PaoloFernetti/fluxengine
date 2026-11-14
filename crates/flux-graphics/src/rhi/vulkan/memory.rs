//! Память Vulkan: выбор типа, аллокации и бюджет VRAM.

#![allow(unsafe_code)]

use std::collections::BTreeMap;

use ash::vk;
use flux_core::memory::MemoryTag;

use super::super::error::RhiError;
use super::super::vram::{BudgetSnapshot, Pressure, VramLease};
use super::convert::map_vk;

/// Одна аллокация `VkDeviceMemory`.
pub(crate) struct Allocation {
    pub(crate) memory: vk::DeviceMemory,
    pub(crate) size: u64,
    /// Память можно отобразить на CPU (staging/readback).
    pub(crate) host_visible: bool,
}

/// Состояние памяти устройства: аллокации ресурсов и лизинги бюджета.
pub(crate) struct MemoryState {
    props: vk::PhysicalDeviceMemoryProperties,
    total: u64,
    allocated: u64,
    lease_bytes: u64,
    next_lease: u64,
    leases: BTreeMap<u64, VramLease>,
}

impl MemoryState {
    /// Считает бюджет по device-local кучам (с откатом на сумму всех куч).
    pub(crate) fn new(props: vk::PhysicalDeviceMemoryProperties) -> Self {
        let mut device_local = 0u64;
        let mut any = 0u64;
        for index in 0..props.memory_heap_count.min(16) as usize {
            let heap = props.memory_heaps[index];
            any = any.saturating_add(heap.size);
            if heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL) {
                device_local = device_local.saturating_add(heap.size);
            }
        }
        Self {
            props,
            total: if device_local > 0 { device_local } else { any },
            allocated: 0,
            lease_bytes: 0,
            next_lease: 0,
            leases: BTreeMap::new(),
        }
    }

    /// Всего байт бюджета.
    #[allow(dead_code)]
    pub(crate) fn total(&self) -> u64 {
        self.total
    }

    /// Занято ресурсами, байт.
    pub(crate) fn allocated(&self) -> u64 {
        self.allocated
    }

    /// Число активных лизингов.
    #[allow(dead_code)]
    pub(crate) fn lease_count(&self) -> usize {
        self.leases.len()
    }

    /// Выделяет память под требования ресурса.
    pub(crate) fn allocate(
        &mut self,
        device: &ash::Device,
        requirements: vk::MemoryRequirements,
        required: vk::MemoryPropertyFlags,
        preferred: vk::MemoryPropertyFlags,
        requested: u64,
    ) -> Result<Allocation, RhiError> {
        let type_index = find_memory_type(
            &self.props,
            requirements.memory_type_bits,
            required,
            preferred,
        )
        .ok_or_else(|| RhiError::validation("no suitable Vulkan memory type"))?;
        let size = requirements.size;
        if self.allocated.saturating_add(size) > self.total {
            return Err(RhiError::OutOfMemory { requested });
        }
        let info = vk::MemoryAllocateInfo::default()
            .allocation_size(size)
            .memory_type_index(type_index);
        // SAFETY: `info` describes a valid allocation request.
        let memory = unsafe { device.allocate_memory(&info, None) }.map_err(map_vk)?;
        self.allocated = self.allocated.saturating_add(size);
        let host_visible = self.props.memory_types[type_index as usize]
            .property_flags
            .contains(vk::MemoryPropertyFlags::HOST_VISIBLE);
        Ok(Allocation {
            memory,
            size,
            host_visible,
        })
    }

    /// Освобождает память ресурса.
    pub(crate) fn free(&mut self, device: &ash::Device, allocation: Allocation) {
        // SAFETY: `allocation.memory` is owned by this state and not used after.
        unsafe { device.free_memory(allocation.memory, None) };
        self.allocated = self.allocated.saturating_sub(allocation.size);
    }

    /// Захватывает бюджет без ресурса (лизинг).
    pub(crate) fn lease(&mut self, bytes: u64, tag: MemoryTag) -> Result<VramLease, RhiError> {
        if bytes == 0 {
            return Err(RhiError::validation("zero-sized VRAM lease"));
        }
        let occupied = self
            .allocated
            .saturating_add(self.lease_bytes)
            .saturating_add(bytes);
        if occupied > self.total {
            return Err(RhiError::OutOfMemory { requested: bytes });
        }
        let lease = VramLease {
            id: self.next_lease,
            bytes,
            tag,
        };
        self.next_lease = self.next_lease.wrapping_add(1);
        self.lease_bytes = self.lease_bytes.saturating_add(bytes);
        self.leases.insert(lease.id, lease);
        Ok(lease)
    }

    /// Освобождает лизинг. `false` — лизинг неизвестен.
    pub(crate) fn free_lease(&mut self, lease: VramLease) -> bool {
        let Some(removed) = self.leases.remove(&lease.id) else {
            return false;
        };
        self.lease_bytes = self.lease_bytes.saturating_sub(removed.bytes);
        true
    }

    /// Снимок бюджета.
    pub(crate) fn budget(&self) -> BudgetSnapshot {
        let used = self.allocated.saturating_add(self.lease_bytes);
        let snapshot = BudgetSnapshot {
            total: self.total,
            reserved: 0,
            used,
            pressure: Pressure::Normal,
        };
        BudgetSnapshot {
            pressure: Pressure::from_ratio(snapshot.utilization()),
            ..snapshot
        }
    }
}

/// Ищет индекс типа памяти: сначала с `preferred`, затем любой с `required`.
pub(crate) fn find_memory_type(
    props: &vk::PhysicalDeviceMemoryProperties,
    type_bits: u32,
    required: vk::MemoryPropertyFlags,
    preferred: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let count = props.memory_type_count.min(32);
    let mut fallback = None;
    for index in 0..count {
        if type_bits & (1 << index) == 0 {
            continue;
        }
        let flags = props.memory_types[index as usize].property_flags;
        if !flags.contains(required) {
            continue;
        }
        if flags.contains(preferred) {
            return Some(index);
        }
        fallback.get_or_insert(index);
    }
    fallback
}
