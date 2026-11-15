//! GPU-геометрия сцены (4.5, ADR-0047): vertex pulling.
//!
//! Вершины и инстансы лежат в device-local storage-буферах и читаются
//! шейдером по `SV_VertexID`/`SV_InstanceID` — vertex buffer не нужен.
//! Загрузка идёт через staging + [`CommandBuffer::copy_buffer_to_buffer`]
//! (ADR-0046), поэтому host-visible остаётся только staging.
//!
//! Инстансы camera-relative: [`RenderScene::render_positions`] уже вычтены
//! позицией камеры, а view-матрица — только поворот. Материалы пока
//! процедурные (палитра-заглушка), пока нет ресурсов материалов.

use flux_math::{Mat4, Vec3, Vec4};

use super::gbuffer::GbufferLayout;
use super::scene::RenderScene;
use crate::rhi::{
    BindingKind, BufferId, BufferUsage, DepthCompare, DescriptorBinding, DescriptorSetDesc,
    DescriptorSetId, DescriptorWrite, GpuBufferDesc, PipelineDesc, PipelineId, RhiDevice, RhiError,
    TextureFormat,
};

const SCENE_VS: &[u8] = include_bytes!("../../shaders/compiled/scene.vert.spv");
const SCENE_GBUFFER_FS: &[u8] = include_bytes!("../../shaders/compiled/scene_gbuffer.frag.spv");
const SHADOW_VS: &[u8] = include_bytes!("../../shaders/compiled/shadow_scene.vert.spv");
const SHADOW_FS: &[u8] = include_bytes!("../../shaders/compiled/shadow.frag.spv");

/// Сторона shadow map (ADR-0050).
pub const SHADOW_MAP_SIZE: u32 = 512;

/// Направление света по умолчанию (camera-relative, сверху-сбоку).
pub const SHADOW_LIGHT_DIR: Vec3 = Vec3::new(0.5, -1.0, 0.3);

/// Полуразмер ортопроекции света вокруг камеры.
pub const SHADOW_EXTENT: f32 = 12.0;

/// Байт на вершину: `position.xyz` + `normal.xyz`.
pub const SCENE_VERTEX_BYTES: u64 = 6 * 4;

/// Байт на инстанс: позиция/масштаб, albedo, roughness, metallic.
pub const SCENE_INSTANCE_BYTES: u64 = 12 * 4;

/// Размер push constants геометрии: view-projection (16 × f32).
pub const SCENE_PUSH_CONSTANT_BYTES: u32 = 64;

/// Форматы G-buffer в порядке RT0..RT2 (ADR-0017).
const GBUFFER_FORMATS: [TextureFormat; 3] = [
    TextureFormat::Rgba8Srgb,
    TextureFormat::Rgba16Float,
    TextureFormat::Rgba8Unorm,
];

const SCENE_BINDINGS: [DescriptorBinding; 2] = [
    DescriptorBinding::new(0, BindingKind::StorageBuffer),
    DescriptorBinding::new(1, BindingKind::StorageBuffer),
];

/// Палитра-заглушка: `material_id % MATERIAL_PALETTE.len()`.
///
/// Заменяется ресурсами материалов, когда появится bindless-таблица.
pub const MATERIAL_PALETTE: [[f32; 3]; 4] = [
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.9, 0.9, 0.9],
];

/// Вершина в GPU-формате.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct SceneVertex {
    position: [f32; 3],
    normal: [f32; 3],
}

/// Инстанс в GPU-формате (совпадает со `SceneInstance` в HLSL).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct SceneInstance {
    position: [f32; 3],
    scale: f32,
    albedo: [f32; 3],
    roughness: f32,
    metallic: f32,
    _padding: [f32; 3],
}

/// Единичный куб (центр в начале координат, полудлина 0.5): 36 вершин.
const CUBE_VERTICES: [SceneVertex; 36] = cube_vertices();

const fn cube_vertices() -> [SceneVertex; 36] {
    // Шесть граней по два треугольника; нормаль — ось грани.
    let faces: [[[f32; 3]; 4]; 6] = [
        // +X
        [
            [0.5, -0.5, -0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [0.5, 0.5, -0.5],
        ],
        // -X
        [
            [-0.5, -0.5, 0.5],
            [-0.5, -0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [-0.5, 0.5, 0.5],
        ],
        // +Y
        [
            [-0.5, 0.5, -0.5],
            [0.5, 0.5, -0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
        ],
        // -Y
        [
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, -0.5, -0.5],
            [-0.5, -0.5, -0.5],
        ],
        // +Z
        [
            [-0.5, -0.5, 0.5],
            [-0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
            [0.5, -0.5, 0.5],
        ],
        // -Z
        [
            [0.5, -0.5, -0.5],
            [0.5, 0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [-0.5, -0.5, -0.5],
        ],
    ];
    let normals: [[f32; 3]; 6] = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let mut vertices = [SceneVertex {
        position: [0.0; 3],
        normal: [0.0; 3],
    }; 36];
    let mut index = 0;
    while index < 36 {
        let face = index / 6;
        let corner = index % 6;
        // Треугольники: 0-1-2, 0-2-3.
        let source = match corner {
            0 | 3 => 0,
            1 => 1,
            2 | 4 => 2,
            _ => 3,
        };
        vertices[index] = SceneVertex {
            position: faces[face][source],
            normal: normals[face],
        };
        index += 1;
    }
    vertices
}

/// GPU-геометрия сцены: буферы, pipeline и набор.
pub struct SceneGeometry {
    vertices: BufferId,
    instances: BufferId,
    staging: BufferId,
    set: DescriptorSetId,
    pipeline: PipelineId,
    shadow_pipeline: PipelineId,
    capacity: usize,
    instance_count: u32,
}

impl SceneGeometry {
    /// Создаёт буферы под `capacity` инстансов и загружает единичный куб.
    ///
    /// # Errors
    ///
    /// Нулевая ёмкость, ошибка создания ресурсов или загрузки вершин.
    pub fn new(device: &mut dyn RhiDevice, capacity: usize) -> Result<Self, RhiError> {
        if capacity == 0 {
            return Err(RhiError::validation("geometry capacity must be non-zero"));
        }
        let vertex_bytes = (CUBE_VERTICES.len() as u64) * SCENE_VERTEX_BYTES;
        let instance_bytes = (capacity as u64) * SCENE_INSTANCE_BYTES;
        let vertices = create_buffer(device, "scene-vertices", vertex_bytes, storage_upload())?;
        let instances = create_buffer(device, "scene-instances", instance_bytes, storage_upload())?;
        let staging = create_buffer(
            device,
            "scene-upload",
            vertex_bytes.max(instance_bytes),
            BufferUsage::staging(),
        )?;

        let mut vertex_data = Vec::with_capacity(vertex_bytes as usize);
        for vertex in &CUBE_VERTICES {
            for value in vertex.position.iter().chain(vertex.normal.iter()) {
                vertex_data.extend_from_slice(&value.to_le_bytes());
            }
        }
        device.write_buffer(staging, 0, &vertex_data)?;
        let mut commands = device.create_command_buffer()?;
        commands.begin()?;
        commands.copy_buffer_to_buffer(staging, vertices, vertex_bytes)?;
        commands.end()?;
        commands.finish()?;

        let layout = GbufferLayout::default();
        let pipeline = device.create_pipeline(
            &PipelineDesc::graphics("scene-gbuffer", SCENE_VS, SCENE_GBUFFER_FS)
                .with_color_formats(&GBUFFER_FORMATS)
                .with_depth_format(layout.depth)
                .with_vertex_stride(0)
                .with_bindings(&SCENE_BINDINGS)
                .with_push_constants(SCENE_PUSH_CONSTANT_BYTES),
        )?;
        // Тени (ADR-0050): depth-only, Reversed-Z — ближе к свету больше.
        let shadow_pipeline = device.create_pipeline(
            &PipelineDesc::graphics("scene-shadow", SHADOW_VS, SHADOW_FS)
                .with_depth_format(TextureFormat::D32Float)
                .with_vertex_stride(0)
                .with_bindings(&SCENE_BINDINGS)
                .with_push_constants(SCENE_PUSH_CONSTANT_BYTES)
                .with_depth_state(DepthCompare::GreaterOrEqual, true),
        )?;
        let set = device.create_descriptor_set(
            &DescriptorSetDesc::new("scene-geometry").with_bindings(&SCENE_BINDINGS),
        )?;
        device.update_descriptor_set(
            set,
            &[
                DescriptorWrite::buffer(0, 0, vertices),
                DescriptorWrite::buffer(1, 0, instances),
            ],
        )?;

        Ok(Self {
            vertices,
            instances,
            staging,
            set,
            pipeline,
            shadow_pipeline,
            capacity,
            instance_count: 0,
        })
    }

    /// Ёмкость инстансов.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Число инстансов последнего [`Self::upload`].
    #[must_use]
    pub fn instance_count(&self) -> u32 {
        self.instance_count
    }

    /// Число вершин единичного меша.
    #[must_use]
    pub fn vertex_count(&self) -> u32 {
        CUBE_VERTICES.len() as u32
    }

    /// Pipeline G-buffer-прохода геометрии.
    #[must_use]
    pub fn pipeline(&self) -> PipelineId {
        self.pipeline
    }

    /// Pipeline depth-only прохода теней (ADR-0050).
    #[must_use]
    pub fn shadow_pipeline(&self) -> PipelineId {
        self.shadow_pipeline
    }

    /// Дескрипторный набор с вершинами и инстансами.
    #[must_use]
    pub fn descriptor_set(&self) -> DescriptorSetId {
        self.set
    }

    /// Загружает видимые инстансы сцены в storage-буфер.
    ///
    /// Использует [`RenderScene::visible_indices`] после `extract`; если
    /// culling не запускался, берутся все инстансы. Позиции — camera-relative.
    ///
    /// # Errors
    ///
    /// Сцена не интерполирована, не помещается в ёмкость или ошибка RHI.
    pub fn upload(
        &mut self,
        device: &mut dyn RhiDevice,
        scene: &RenderScene,
    ) -> Result<(), RhiError> {
        let positions = scene.render_positions();
        if positions.is_empty() && !scene.is_empty() {
            return Err(RhiError::validation(
                "scene must be interpolated or extracted before upload",
            ));
        }
        let visible = scene.visible_indices();
        let indices: Vec<u32> = if visible.is_empty() {
            (0..positions.len() as u32).collect()
        } else {
            visible.to_vec()
        };
        if indices.len() > self.capacity {
            return Err(RhiError::validation("scene exceeds geometry capacity"));
        }
        let materials = scene.material_ids();
        let mut bytes = Vec::with_capacity(indices.len() * SCENE_INSTANCE_BYTES as usize);
        for index in &indices {
            let position = positions
                .get(*index as usize)
                .copied()
                .unwrap_or_default()
                .to_array();
            let material = materials.get(*index as usize).copied().unwrap_or(0);
            let instance = SceneInstance::from_material(position, material);
            for value in instance
                .position
                .iter()
                .chain(std::iter::once(&instance.scale))
                .chain(instance.albedo.iter())
                .chain(std::iter::once(&instance.roughness))
                .chain(std::iter::once(&instance.metallic))
                .chain(instance._padding.iter())
            {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        if !bytes.is_empty() {
            device.write_buffer(self.staging, 0, &bytes)?;
            let mut commands = device.create_command_buffer()?;
            commands.begin()?;
            commands.copy_buffer_to_buffer(self.staging, self.instances, bytes.len() as u64)?;
            commands.end()?;
            commands.finish()?;
        }
        self.instance_count = indices.len() as u32;
        Ok(())
    }

    /// Освобождает pipeline, набор и буферы.
    pub fn destroy(self, device: &mut dyn RhiDevice) {
        let _ = device.destroy_pipeline(self.pipeline);
        let _ = device.destroy_pipeline(self.shadow_pipeline);
        let _ = device.destroy_descriptor_set(self.set);
        let _ = device.destroy_buffer(self.vertices);
        let _ = device.destroy_buffer(self.instances);
        let _ = device.destroy_buffer(self.staging);
    }
}

impl SceneInstance {
    /// Инстанс из camera-relative позиции и id материала (палитра-заглушка).
    fn from_material(position: [f32; 3], material: u32) -> Self {
        let slot = (material as usize) % MATERIAL_PALETTE.len();
        Self {
            position,
            scale: 1.0,
            albedo: MATERIAL_PALETTE[slot],
            roughness: 0.4 + 0.15 * (material % 3) as f32,
            metallic: if material % 2 == 0 { 0.0 } else { 1.0 },
            _padding: [0.0; 3],
        }
    }
}

/// Camera-relative light view-projection для shadow map (ADR-0050).
///
/// Инстансы уже вычтены позицией камеры, поэтому ортопроекция строится
/// вокруг начала координат: при уходе камеры от world origin точность не
/// теряется. Глубина — Reversed-Z (`near → 1`, `far → 0`), как у камеры
/// (ADR-0008): RHI очищает depth в `0.0`, поэтому PSO теней использует
/// `GreaterOrEqual`, а ближе к свету — больше z.
#[must_use]
pub fn shadow_view_projection(light_dir: Vec3) -> Mat4 {
    let direction = if light_dir.length_squared() > 0.0 {
        light_dir.normalize()
    } else {
        -Vec3::Y
    };
    // При взгляде строго вдоль Y опорный вектор Y вырожден.
    let up = if direction.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let eye = -direction * SHADOW_EXTENT;
    let view = Mat4::look_at_rh(eye, Vec3::ZERO, up);
    // Reversed-Z ортопроекция: x/y ∈ [-1, 1], z = z_view / (2E) + 1.
    let xy = 1.0 / SHADOW_EXTENT;
    let z = 1.0 / (SHADOW_EXTENT * 2.0);
    let projection = Mat4::from_cols(
        Vec4::new(xy, 0.0, 0.0, 0.0),
        Vec4::new(0.0, xy, 0.0, 0.0),
        Vec4::new(0.0, 0.0, z, 0.0),
        Vec4::new(0.0, 0.0, 1.0, 1.0),
    );
    projection * view
}

/// Push constants геометрии: view-projection (column-major).
#[must_use]
pub fn scene_push_constants(view_projection: &Mat4) -> [u8; SCENE_PUSH_CONSTANT_BYTES as usize] {
    let mut bytes = [0u8; SCENE_PUSH_CONSTANT_BYTES as usize];
    for (index, value) in view_projection.to_cols_array().iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Storage-буфер, заполняемый копированием из staging (ADR-0046).
const fn storage_upload() -> BufferUsage {
    BufferUsage {
        storage: true,
        transfer_dst: true,
        ..BufferUsage::none()
    }
}

fn create_buffer(
    device: &mut dyn RhiDevice,
    label: &'static str,
    size: u64,
    usage: BufferUsage,
) -> Result<BufferId, RhiError> {
    device.create_buffer(&GpuBufferDesc::new(label, size, usage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rhi::NullDevice;
    use flux_math::Vec3;

    #[test]
    fn unit_cube_has_axis_normals() {
        assert_eq!(CUBE_VERTICES.len(), 36);
        for face in 0..6 {
            let normal = CUBE_VERTICES[face * 6].normal;
            assert_eq!(normal.iter().map(|value| value.abs()).sum::<f32>(), 1.0);
            for vertex in &CUBE_VERTICES[face * 6..face * 6 + 6] {
                assert_eq!(vertex.normal, normal);
                assert!(vertex.position.iter().all(|value| value.abs() == 0.5));
            }
        }
    }

    #[test]
    fn push_constants_are_column_major_view_projection() {
        let matrix = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let bytes = scene_push_constants(&matrix);
        assert_eq!(bytes.len(), SCENE_PUSH_CONSTANT_BYTES as usize);
        let columns = matrix.to_cols_array();
        for (index, value) in columns.iter().enumerate() {
            let offset = index * 4;
            let decoded = f32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            assert_eq!(decoded, *value);
        }
    }

    #[test]
    fn shadow_view_projection_covers_origin() {
        let view_projection = shadow_view_projection(SHADOW_LIGHT_DIR);
        let ndc = view_projection.project_point3(Vec3::ZERO);
        assert!(ndc.x.abs() < 1.0, "x={}", ndc.x);
        assert!(ndc.y.abs() < 1.0, "y={}", ndc.y);
        assert!(ndc.z > 0.0 && ndc.z < 1.0, "z={}", ndc.z);
        // Матрица не зависит от позиции камеры: инстансы camera-relative.
        // Дальше от света = смещение вдоль направления света (Reversed-Z:
        // дальше — меньше z).
        let farther = view_projection.project_point3(SHADOW_LIGHT_DIR.normalize());
        assert!(farther.z < ndc.z, "дальше от света — меньше z");
    }

    #[test]
    fn upload_rejects_capacity_overflow() {
        let mut device = NullDevice::new();
        let mut geometry = SceneGeometry::new(&mut device, 1).unwrap();
        assert_eq!(geometry.capacity(), 1);
        assert_eq!(geometry.vertex_count(), 36);
        let mut scene = RenderScene::new();
        scene.push(Vec3::ZERO, Vec3::ZERO, 0, 0);
        scene.push(Vec3::X, Vec3::X, 0, 1);
        scene.interpolate(1.0);
        assert!(geometry.upload(&mut device, &scene).is_err());
        geometry.destroy(&mut device);
    }

    #[test]
    fn upload_requires_interpolated_scene() {
        let mut device = NullDevice::new();
        let mut geometry = SceneGeometry::new(&mut device, 4).unwrap();
        let mut scene = RenderScene::new();
        scene.push(Vec3::ZERO, Vec3::ZERO, 0, 0);
        assert!(geometry.upload(&mut device, &scene).is_err());
        scene.interpolate(1.0);
        geometry.upload(&mut device, &scene).unwrap();
        assert_eq!(geometry.instance_count(), 1);
        geometry.destroy(&mut device);
    }
}
