//! Render Graph: пассы, автоматические барьеры, aliasing, rebuild по resize.
//!
//! Граф не знает конкретного API: он планирует порядок пассов и переходы
//! состояний изображений (`ImageState`), а RHI транслирует их в барьеры.
//! Добавление пасса не требует ручных барьеров; повторный resize
//! коалесцируется в один rebuild.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use super::super::rhi::{BarrierBatch, CommandBuffer, ImageId, ImageState, RhiError};

/// Ключ изображения графа (совпадает с id ресурса в реестре).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphImageKey(pub u64);

/// Описание изображения графа.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphImage {
    /// Ключ.
    pub key: GraphImageKey,
    /// Оценка размера, байт (для aliasing).
    pub bytes: u64,
    /// Внешний ресурс: не участвует в aliasing.
    pub external: bool,
}

impl GraphImage {
    /// Transient-изображение.
    #[must_use]
    pub const fn new(key: GraphImageKey, bytes: u64) -> Self {
        Self {
            key,
            bytes,
            external: false,
        }
    }

    /// Внешний ресурс (swapchain image, импорт).
    #[must_use]
    pub const fn external(key: GraphImageKey, bytes: u64) -> Self {
        Self {
            key,
            bytes,
            external: true,
        }
    }
}

/// Тип пасса.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PassKind {
    /// Графический пасс (G-buffer, lighting, post).
    Graphics,
    /// Compute-пасс.
    Compute,
    /// Копирование/transfer.
    Transfer,
    /// Представление.
    Present,
}

/// Пасс графа.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphPass {
    /// Имя для диагностики и детерминированного порядка.
    pub name: &'static str,
    /// Тип.
    pub kind: PassKind,
    /// Читаемые изображения.
    pub reads: Vec<GraphImageKey>,
    /// Записываемые изображения (не аттачменты).
    pub writes: Vec<GraphImageKey>,
    /// Цветовые аттачменты.
    pub color_attachments: Vec<GraphImageKey>,
    /// Depth-аттачмент.
    pub depth_attachment: Option<GraphImageKey>,
}

impl GraphPass {
    /// Пустой пасс.
    #[must_use]
    pub fn new(name: &'static str, kind: PassKind) -> Self {
        Self {
            name,
            kind,
            reads: Vec::new(),
            writes: Vec::new(),
            color_attachments: Vec::new(),
            depth_attachment: None,
        }
    }

    /// Добавляет чтение.
    #[must_use]
    pub fn read(mut self, key: GraphImageKey) -> Self {
        self.reads.push(key);
        self
    }

    /// Добавляет запись (storage/transfer).
    #[must_use]
    pub fn write(mut self, key: GraphImageKey) -> Self {
        self.writes.push(key);
        self
    }

    /// Добавляет цветовой аттачмент.
    #[must_use]
    pub fn color(mut self, key: GraphImageKey) -> Self {
        self.color_attachments.push(key);
        self
    }

    /// Задаёт depth-аттачмент.
    #[must_use]
    pub fn depth(mut self, key: GraphImageKey) -> Self {
        self.depth_attachment = Some(key);
        self
    }

    fn written(&self) -> Vec<GraphImageKey> {
        let mut written = self.writes.clone();
        written.extend_from_slice(&self.color_attachments);
        if let Some(depth) = self.depth_attachment {
            written.push(depth);
        }
        written.sort_unstable();
        written.dedup();
        written
    }

    fn affected(&self) -> Vec<GraphImageKey> {
        let mut affected = self.reads.clone();
        affected.extend(self.written());
        affected.sort_unstable();
        affected.dedup();
        affected
    }
}

/// Ошибка построения графа.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum GraphError {
    /// Изображение зарегистрировано дважды.
    #[error("duplicate graph image: {0:?}")]
    DuplicateImage(GraphImageKey),
    /// Имя пасса повторяется.
    #[error("duplicate pass name: {0}")]
    DuplicatePass(&'static str),
    /// Пасс ссылается на незарегистрированное изображение.
    #[error("pass '{pass}' references unknown image {image:?}")]
    UnknownImage {
        /// Имя пасса.
        pass: &'static str,
        /// Ключ изображения.
        image: GraphImageKey,
    },
    /// Циклическая зависимость.
    #[error("render graph has a dependency cycle")]
    Cycle,
}

/// Скомпилированный пасс: барьеры и список ресурсов.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledPass {
    /// Имя.
    pub name: &'static str,
    /// Тип.
    pub kind: PassKind,
    /// Барьеры, которые нужно применить перед пассом.
    pub barriers: BarrierBatch,
    /// Прочитанные изображения.
    pub reads: Vec<GraphImageKey>,
    /// Записанные изображения.
    pub writes: Vec<GraphImageKey>,
}

/// Слот aliasing: изображения с непересекающимся временем жизни.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AliasSlot {
    /// Ёмкость слота, байт (максимум из участников).
    pub bytes: u64,
    /// Участники, по возрастанию ключа.
    pub members: Vec<GraphImageKey>,
}

/// Результат компиляции графа.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderGraphPlan {
    /// Пассы в порядке исполнения.
    pub passes: Vec<CompiledPass>,
    /// План переиспользования памяти transient-ресурсов.
    pub alias_slots: Vec<AliasSlot>,
    /// Имена пассов в порядке исполнения.
    pub order: Vec<&'static str>,
}

impl RenderGraphPlan {
    /// Суммарное число барьеров плана.
    #[must_use]
    pub fn barrier_count(&self) -> usize {
        self.passes.iter().map(|pass| pass.barriers.len()).sum()
    }
}

/// Render Graph.
#[derive(Clone, Debug, Default)]
pub struct RenderGraph {
    images: BTreeMap<GraphImageKey, GraphImage>,
    passes: Vec<GraphPass>,
    output_size: (u32, u32),
    dirty: bool,
}

impl RenderGraph {
    /// Пустой граф.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует изображение.
    ///
    /// # Errors
    ///
    /// [`GraphError::DuplicateImage`] — ключ уже занят.
    pub fn add_image(&mut self, image: GraphImage) -> Result<(), GraphError> {
        if self.images.contains_key(&image.key) {
            return Err(GraphError::DuplicateImage(image.key));
        }
        self.images.insert(image.key, image);
        self.dirty = true;
        Ok(())
    }

    /// Добавляет пасс.
    ///
    /// # Errors
    ///
    /// [`GraphError::DuplicatePass`] — имя уже используется.
    pub fn add_pass(&mut self, pass: GraphPass) -> Result<(), GraphError> {
        if self
            .passes
            .iter()
            .any(|existing| existing.name == pass.name)
        {
            return Err(GraphError::DuplicatePass(pass.name));
        }
        self.passes.push(pass);
        self.dirty = true;
        Ok(())
    }

    /// Изображения графа.
    #[must_use]
    pub fn images(&self) -> &BTreeMap<GraphImageKey, GraphImage> {
        &self.images
    }

    /// Пассы в порядке добавления.
    #[must_use]
    pub fn passes(&self) -> &[GraphPass] {
        &self.passes
    }

    /// Выходной размер.
    #[must_use]
    pub fn output_size(&self) -> (u32, u32) {
        self.output_size
    }

    /// Меняет выходной размер. Возвращает `true`, если размер изменился.
    /// Нулевые размеры игнорируются.
    pub fn set_output_size(&mut self, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 {
            return false;
        }
        if self.output_size == (width, height) {
            return false;
        }
        self.output_size = (width, height);
        self.dirty = true;
        true
    }

    /// Нужен ли rebuild (resize/preset).
    #[must_use]
    pub fn needs_rebuild(&self) -> bool {
        self.dirty
    }

    /// Сбрасывает флаг rebuild и возвращает, был ли он установлен.
    pub fn rebuild_if_needed(&mut self) -> bool {
        let dirty = self.dirty;
        self.dirty = false;
        dirty
    }

    /// Компилирует граф: топологический порядок, барьеры, aliasing.
    ///
    /// # Errors
    ///
    /// [`GraphError::UnknownImage`] — пасс ссылается на неизвестный ресурс;
    /// [`GraphError::Cycle`] — зависимости образуют цикл.
    pub fn compile(&self) -> Result<RenderGraphPlan, GraphError> {
        self.compile_with_initial(&BTreeMap::new())
    }

    /// Компилирует граф с известными начальными состояниями изображений.
    ///
    /// Нужен для внешних ресурсов, переживающих кадр (история TAA,
    /// swapchain): переход из `Undefined` стирает содержимое, поэтому
    /// `oldLayout` обязан совпадать с фактическим состоянием.
    ///
    /// # Errors
    ///
    /// Те же, что у [`Self::compile`].
    pub fn compile_with_initial(
        &self,
        initial: &BTreeMap<GraphImageKey, ImageState>,
    ) -> Result<RenderGraphPlan, GraphError> {
        let count = self.passes.len();
        let written: Vec<Vec<GraphImageKey>> = self.passes.iter().map(GraphPass::written).collect();
        let affected: Vec<Vec<GraphImageKey>> =
            self.passes.iter().map(GraphPass::affected).collect();
        for pass in &self.passes {
            for key in pass.affected() {
                if !self.images.contains_key(&key) {
                    return Err(GraphError::UnknownImage {
                        pass: pass.name,
                        image: key,
                    });
                }
            }
        }

        let mut edges = vec![BTreeSet::new(); count];
        for &key in self.images.keys() {
            let writers: Vec<usize> = (0..count)
                .filter(|&index| written[index].contains(&key))
                .collect();
            let readers: Vec<usize> = (0..count)
                .filter(|&index| {
                    self.passes[index].reads.contains(&key) && !written[index].contains(&key)
                })
                .collect();
            for pair in writers.windows(2) {
                edges[pair[0]].insert(pair[1]);
            }
            for &writer in &writers {
                for &reader in &readers {
                    if writer != reader {
                        edges[writer].insert(reader);
                    }
                }
            }
        }

        let order = topological_order(&edges)?;
        let mut states: BTreeMap<GraphImageKey, ImageState> = self
            .images
            .keys()
            .map(|&key| {
                (
                    key,
                    initial.get(&key).copied().unwrap_or(ImageState::Undefined),
                )
            })
            .collect();
        let mut passes = Vec::with_capacity(count);
        for &index in &order {
            let pass = &self.passes[index];
            let mut barriers = BarrierBatch::new();
            for &key in &affected[index] {
                let target = target_state(pass, key);
                let current = states[&key];
                if current != target {
                    barriers.transition(ImageId(key.0), current, target);
                    states.insert(key, target);
                }
            }
            passes.push(CompiledPass {
                name: pass.name,
                kind: pass.kind,
                barriers,
                reads: pass.reads.clone(),
                writes: written[index].clone(),
            });
        }

        let alias_slots = alias_plan(&self.images, &affected, &order);
        Ok(RenderGraphPlan {
            passes,
            alias_slots,
            order: order.iter().map(|&index| self.passes[index].name).collect(),
        })
    }

    /// Применяет барьеры плана к командному буферу.
    ///
    /// Тела пассов (draw/dispatch и привязка ресурсов) передаются хуком
    /// [`Self::execute_with`]; здесь фиксируется только синхронизация, чтобы
    /// Render Graph не зависел от API.
    ///
    /// # Errors
    ///
    /// Ошибка командного буфера (например, запись не начата).
    pub fn execute(
        &self,
        plan: &RenderGraphPlan,
        commands: &mut dyn CommandBuffer,
    ) -> Result<(), RhiError> {
        self.execute_with(plan, commands, |_, _| Ok(()))
    }

    /// Применяет барьеры и вызывает тело пасса для каждого узла плана.
    ///
    /// Хук получает скомпилированный пасс (имя, тип, барьеры) и командный
    /// буфер; порядок вызовов детерминирован порядком плана. Граф по-прежнему
    /// не знает конкретного API — тела пассов живут в рендерере.
    ///
    /// # Errors
    ///
    /// Ошибка командного буфера или тела пасса.
    pub fn execute_with(
        &self,
        plan: &RenderGraphPlan,
        commands: &mut dyn CommandBuffer,
        mut body: impl FnMut(&CompiledPass, &mut dyn CommandBuffer) -> Result<(), RhiError>,
    ) -> Result<(), RhiError> {
        for pass in &plan.passes {
            commands.barriers(&pass.barriers)?;
            body(pass, commands)?;
        }
        Ok(())
    }
}

fn target_state(pass: &GraphPass, key: GraphImageKey) -> ImageState {
    if pass.depth_attachment == Some(key) {
        return ImageState::DepthAttachment;
    }
    if pass.color_attachments.contains(&key) {
        return ImageState::ColorAttachment;
    }
    if pass.writes.contains(&key) {
        return match pass.kind {
            PassKind::Graphics | PassKind::Compute => ImageState::Storage,
            PassKind::Transfer => ImageState::TransferDst,
            PassKind::Present => ImageState::Present,
        };
    }
    match pass.kind {
        PassKind::Transfer => ImageState::TransferSrc,
        PassKind::Graphics | PassKind::Compute | PassKind::Present => ImageState::ShaderRead,
    }
}

fn topological_order(edges: &[BTreeSet<usize>]) -> Result<Vec<usize>, GraphError> {
    let mut indegree = vec![0usize; edges.len()];
    for targets in edges {
        for &target in targets {
            indegree[target] += 1;
        }
    }
    let mut ready: BTreeSet<usize> = indegree
        .iter()
        .enumerate()
        .filter(|(_, degree)| **degree == 0)
        .map(|(index, _)| index)
        .collect();
    let mut order = Vec::with_capacity(edges.len());
    while let Some(&index) = ready.iter().next() {
        ready.remove(&index);
        order.push(index);
        for &target in &edges[index] {
            indegree[target] -= 1;
            if indegree[target] == 0 {
                ready.insert(target);
            }
        }
    }
    if order.len() != edges.len() {
        return Err(GraphError::Cycle);
    }
    Ok(order)
}

fn alias_plan(
    images: &BTreeMap<GraphImageKey, GraphImage>,
    affected: &[Vec<GraphImageKey>],
    order: &[usize],
) -> Vec<AliasSlot> {
    let mut lifetimes: BTreeMap<GraphImageKey, (usize, usize)> = BTreeMap::new();
    for (position, &index) in order.iter().enumerate() {
        for &key in &affected[index] {
            lifetimes
                .entry(key)
                .and_modify(|(_, last)| *last = position)
                .or_insert((position, position));
        }
    }
    struct Slot {
        bytes: u64,
        members: Vec<(GraphImageKey, (usize, usize))>,
    }
    let mut slots: Vec<Slot> = Vec::new();
    for (&key, image) in images {
        if image.external || image.bytes == 0 {
            continue;
        }
        let Some(&(first, last)) = lifetimes.get(&key) else {
            continue;
        };
        let mut placed = false;
        for slot in &mut slots {
            if image.bytes > slot.bytes {
                continue;
            }
            let overlaps = slot.members.iter().any(|(_, (member_first, member_last))| {
                !(last < *member_first || first > *member_last)
            });
            if overlaps {
                continue;
            }
            slot.members.push((key, (first, last)));
            placed = true;
            break;
        }
        if !placed {
            slots.push(Slot {
                bytes: image.bytes,
                members: vec![(key, (first, last))],
            });
        }
    }
    slots
        .into_iter()
        .map(|slot| AliasSlot {
            bytes: slot.bytes,
            members: slot.members.into_iter().map(|(key, _)| key).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rhi::{CommandBuffer, NullCommandBuffer};

    fn key(value: u64) -> GraphImageKey {
        GraphImageKey(value)
    }

    fn graph_with_two_passes() -> RenderGraph {
        let mut graph = RenderGraph::new();
        graph.add_image(GraphImage::new(key(1), 1024)).unwrap();
        graph.add_image(GraphImage::new(key(2), 1024)).unwrap();
        graph
            .add_pass(GraphPass::new("gbuffer", PassKind::Graphics).color(key(1)))
            .unwrap();
        graph
            .add_pass(
                GraphPass::new("lighting", PassKind::Graphics)
                    .read(key(1))
                    .color(key(2)),
            )
            .unwrap();
        graph
    }

    #[test]
    fn passes_are_ordered_by_dependencies() {
        let mut graph = RenderGraph::new();
        graph.add_image(GraphImage::new(key(1), 16)).unwrap();
        graph
            .add_pass(
                GraphPass::new("lighting", PassKind::Graphics)
                    .read(key(1))
                    .color(key(2)),
            )
            .unwrap();
        graph.add_image(GraphImage::new(key(2), 16)).unwrap();
        graph
            .add_pass(GraphPass::new("gbuffer", PassKind::Graphics).color(key(1)))
            .unwrap();
        let plan = graph.compile().unwrap();
        assert_eq!(plan.order, vec!["gbuffer", "lighting"]);
    }

    #[test]
    fn barriers_are_derived_from_reads_and_writes() {
        let graph = graph_with_two_passes();
        let plan = graph.compile().unwrap();
        assert_eq!(plan.order, vec!["gbuffer", "lighting"]);
        let gbuffer = &plan.passes[0].barriers.barriers();
        assert_eq!(gbuffer.len(), 1);
        assert_eq!(gbuffer[0].image, ImageId(1));
        assert_eq!(gbuffer[0].from, ImageState::Undefined);
        assert_eq!(gbuffer[0].to, ImageState::ColorAttachment);
        let lighting = &plan.passes[1].barriers.barriers();
        assert_eq!(lighting.len(), 2);
        assert!(lighting.iter().any(|barrier| {
            barrier.image == ImageId(1)
                && barrier.from == ImageState::ColorAttachment
                && barrier.to == ImageState::ShaderRead
        }));
    }

    #[test]
    fn initial_states_avoid_undefined_old_layout() {
        let graph = graph_with_two_passes();
        let mut initial = BTreeMap::new();
        initial.insert(key(1), ImageState::ShaderRead);
        let plan = graph.compile_with_initial(&initial).unwrap();
        let barrier = plan.passes[0]
            .barriers
            .barriers()
            .iter()
            .find(|barrier| barrier.image == ImageId(1))
            .expect("image 1 must be transitioned");
        assert_eq!(barrier.from, ImageState::ShaderRead);
        assert_eq!(barrier.to, ImageState::ColorAttachment);
    }

    #[test]
    fn cycle_is_detected() {
        let mut graph = RenderGraph::new();
        graph.add_image(GraphImage::new(key(1), 16)).unwrap();
        graph.add_image(GraphImage::new(key(2), 16)).unwrap();
        graph
            .add_pass(
                GraphPass::new("a", PassKind::Compute)
                    .read(key(1))
                    .write(key(2)),
            )
            .unwrap();
        graph
            .add_pass(
                GraphPass::new("b", PassKind::Compute)
                    .read(key(2))
                    .write(key(1)),
            )
            .unwrap();
        assert_eq!(graph.compile(), Err(GraphError::Cycle));
    }

    #[test]
    fn unknown_images_and_duplicates_are_rejected() {
        let mut graph = RenderGraph::new();
        graph.add_image(GraphImage::new(key(1), 16)).unwrap();
        assert_eq!(
            graph.add_image(GraphImage::new(key(1), 16)),
            Err(GraphError::DuplicateImage(key(1)))
        );
        graph
            .add_pass(GraphPass::new("a", PassKind::Compute).read(key(9)))
            .unwrap();
        assert!(matches!(
            graph.compile(),
            Err(GraphError::UnknownImage { .. })
        ));
        assert_eq!(
            graph.add_pass(GraphPass::new("a", PassKind::Compute)),
            Err(GraphError::DuplicatePass("a"))
        );
    }

    #[test]
    fn aliasing_reuses_memory_for_disjoint_lifetimes() {
        let mut graph = RenderGraph::new();
        graph.add_image(GraphImage::new(key(1), 1024)).unwrap();
        graph.add_image(GraphImage::new(key(2), 1024)).unwrap();
        graph.add_image(GraphImage::new(key(3), 1024)).unwrap();
        graph.add_image(GraphImage::external(key(4), 1024)).unwrap();
        graph
            .add_pass(
                GraphPass::new("first", PassKind::Graphics)
                    .color(key(1))
                    .color(key(3)),
            )
            .unwrap();
        graph
            .add_pass(
                GraphPass::new("second", PassKind::Graphics)
                    .read(key(3))
                    .color(key(2))
                    .color(key(4)),
            )
            .unwrap();
        let plan = graph.compile().unwrap();
        assert_eq!(plan.alias_slots.len(), 2);
        assert_eq!(plan.alias_slots[0].members, vec![key(1), key(2)]);
        assert_eq!(plan.alias_slots[1].members, vec![key(3)]);
        assert!(plan
            .alias_slots
            .iter()
            .all(|slot| !slot.members.contains(&key(4))));
    }

    #[test]
    fn resize_is_coalesced_into_one_rebuild() {
        let mut graph = RenderGraph::new();
        assert!(!graph.needs_rebuild());
        assert!(graph.set_output_size(1280, 720));
        assert!(graph.set_output_size(1920, 1080));
        assert!(!graph.set_output_size(1920, 1080));
        assert!(!graph.set_output_size(0, 0));
        assert!(graph.needs_rebuild());
        assert!(graph.rebuild_if_needed());
        assert!(!graph.rebuild_if_needed());
        assert_eq!(graph.output_size(), (1920, 1080));
    }

    #[test]
    fn plan_is_deterministic() {
        let first = graph_with_two_passes().compile().unwrap();
        let second = graph_with_two_passes().compile().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn execute_applies_plan_barriers() {
        let graph = graph_with_two_passes();
        let plan = graph.compile().unwrap();
        let mut commands = NullCommandBuffer::new();
        commands.begin().unwrap();
        graph.execute(&plan, &mut commands).unwrap();
        commands.end().unwrap();
        assert_eq!(commands.barriers_applied(), plan.barrier_count());
    }
}
