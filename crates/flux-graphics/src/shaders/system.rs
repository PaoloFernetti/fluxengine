//! Маски keywords, ключи пермутаций, off-demand компиляция и кэш.
//!
//! Три уровня кэша (ADR-0014): байткод (`hash(исходник + keywords)`), PSO
//! (`hash(байткод + state + форматы)`) и материал
//! (`(shader_uuid, material-маска)`). Ядро прекомпилируется заранее и служит
//! fallback-ом на время компиляции остальных вариантов.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{KeywordSet, ShaderDeclaration, ShaderError};
use crate::rhi::PipelineId;

/// FNV-1a: стабильный между процессами и платформами.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a(bytes: impl Iterator<Item = u8>) -> u64 {
    bytes.fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

/// Битовая маска keywords: бит = индекс в декларации.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeywordMask {
    bits: u64,
}

impl KeywordMask {
    /// Пустая маска.
    #[must_use]
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Маска из готовых битов.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self { bits }
    }

    /// Биты маски.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.bits
    }

    /// Пуста ли маска.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.bits == 0
    }

    /// Число установленных битов.
    #[must_use]
    pub fn len(self) -> u32 {
        self.bits.count_ones()
    }

    /// Установлен ли бит `index` (вне `0..64` — `false`).
    #[must_use]
    pub const fn contains(self, index: usize) -> bool {
        index < 64 && (self.bits & (1_u64 << index)) != 0
    }

    /// Маска с установленным битом `index` (вне `0..64` — без изменений).
    #[must_use]
    pub fn with(mut self, index: usize) -> Self {
        if index < 64 {
            self.bits |= 1_u64 << index;
        }
        self
    }

    /// Объединение масок.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    /// Есть ли общий бит.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        (self.bits & other.bits) != 0
    }

    /// Строит маску из имён keywords указанного набора.
    ///
    /// # Errors
    ///
    /// [`ShaderError::UnknownKeyword`] — имени нет в декларации или оно
    /// относится к другому набору.
    pub fn from_names(
        declaration: &ShaderDeclaration,
        set: KeywordSet,
        names: &[&str],
    ) -> Result<Self, ShaderError> {
        let mut mask = Self::empty();
        for name in names {
            let index = declaration
                .keyword_index(name)
                .ok_or_else(|| ShaderError::UnknownKeyword((*name).to_owned()))?;
            if declaration.keywords[index].set != set {
                return Err(ShaderError::UnknownKeyword((*name).to_owned()));
            }
            mask = mask.with(index);
        }
        Ok(mask)
    }
}

/// Запрос на пермутацию: полный ключ `pipeline × mesh × material`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PermutationRequest {
    /// UUID базового шейдера.
    pub shader_uuid: u128,
    /// Индекс pipeline-варианта.
    pub pipeline_variant: u32,
    /// Mesh-keywords.
    pub mesh: KeywordMask,
    /// Material-keywords.
    pub material: KeywordMask,
}

impl PermutationRequest {
    /// Запрос ядра: без mesh/material keywords.
    #[must_use]
    pub const fn new(shader_uuid: u128, pipeline_variant: u32) -> Self {
        Self {
            shader_uuid,
            pipeline_variant,
            mesh: KeywordMask::empty(),
            material: KeywordMask::empty(),
        }
    }

    /// Задаёт mesh-маску.
    #[must_use]
    pub const fn with_mesh(mut self, mesh: KeywordMask) -> Self {
        self.mesh = mesh;
        self
    }

    /// Задаёт material-маску.
    #[must_use]
    pub const fn with_material(mut self, material: KeywordMask) -> Self {
        self.material = material;
        self
    }

    /// Детерминированный ключ байткода: FNV-1a по UUID, варианту и маскам.
    #[must_use]
    pub fn hash(&self) -> u64 {
        let bytes = self
            .shader_uuid
            .to_le_bytes()
            .into_iter()
            .chain(self.pipeline_variant.to_le_bytes())
            .chain(self.mesh.bits().to_le_bytes())
            .chain(self.material.bits().to_le_bytes());
        fnv1a(bytes)
    }
}

/// Компилятор шейдеров (DXC-пайплайн — I6b).
pub trait ShaderCompiler: Send + Sync {
    /// Компилирует пермутацию в байткод (SPIR-V).
    ///
    /// # Errors
    ///
    /// [`ShaderError::CompileFailed`] — компилятор не смог собрать вариант;
    /// библиотека в этом случае отдаёт fallback.
    fn compile(
        &self,
        declaration: &ShaderDeclaration,
        request: &PermutationRequest,
    ) -> Result<Vec<u8>, ShaderError>;
}

/// Детерминированная заглушка компилятора до подключения DXC (I6b).
///
/// Байткод **не является** валидным SPIR-V: он нужен для тестов кэша,
/// fallback-пути и детерминизма ключей.
#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicCompiler;

impl ShaderCompiler for DeterministicCompiler {
    fn compile(
        &self,
        declaration: &ShaderDeclaration,
        request: &PermutationRequest,
    ) -> Result<Vec<u8>, ShaderError> {
        let mut bytecode = Vec::with_capacity(24);
        bytecode.extend_from_slice(b"SCSP");
        bytecode.extend_from_slice(&declaration.shader_uuid.to_le_bytes());
        bytecode.extend_from_slice(&request.hash().to_le_bytes());
        Ok(bytecode)
    }
}

/// Статистика библиотеки: попадания, промахи, компиляции, fallback-и.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Попадания в кэш байткода.
    pub hits: u64,
    /// Промахи.
    pub misses: u64,
    /// Успешные компиляции.
    pub compilations: u64,
    /// Отдачи fallback-а.
    pub fallbacks: u64,
    /// Отказы из-за бюджета пермутаций.
    pub budget_exceeded: u64,
}

/// Трёхуровневый кэш пермутаций.
#[derive(Clone, Debug, Default)]
pub struct PermutationCache {
    bytecode: BTreeMap<u64, Arc<[u8]>>,
    material: BTreeMap<(u128, u64), u64>,
    pso: BTreeMap<u64, PipelineId>,
}

impl PermutationCache {
    /// Пустой кэш.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Байткод по ключу.
    #[must_use]
    pub fn bytecode(&self, key: u64) -> Option<Arc<[u8]>> {
        self.bytecode.get(&key).cloned()
    }

    /// Вставляет байткод; возвращает `true`, если ключ новый.
    pub fn insert_bytecode(&mut self, key: u64, bytecode: Vec<u8>) -> bool {
        self.bytecode.insert(key, Arc::from(bytecode)).is_none()
    }

    /// Число живых байткодов.
    #[must_use]
    pub fn bytecode_len(&self) -> usize {
        self.bytecode.len()
    }

    /// Material-уровень: маска материала → ключ байткода.
    #[must_use]
    pub fn material_entry(&self, shader_uuid: u128, mask: KeywordMask) -> Option<u64> {
        self.material.get(&(shader_uuid, mask.bits())).copied()
    }

    /// Записывает material-уровень.
    pub fn insert_material(&mut self, shader_uuid: u128, mask: KeywordMask, key: u64) {
        self.material.insert((shader_uuid, mask.bits()), key);
    }

    /// PSO-уровень: ключ состояния → хэндл pipeline.
    #[must_use]
    pub fn pso(&self, key: u64) -> Option<PipelineId> {
        self.pso.get(&key).copied()
    }

    /// Записывает PSO-уровень.
    pub fn insert_pso(&mut self, key: u64, pipeline: PipelineId) {
        self.pso.insert(key, pipeline);
    }

    /// Число PSO в кэше.
    #[must_use]
    pub fn pso_len(&self) -> usize {
        self.pso.len()
    }

    /// Очищает все уровни.
    pub fn clear(&mut self) {
        self.bytecode.clear();
        self.material.clear();
        self.pso.clear();
    }
}

/// Результат разрешения пермутации.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedShader {
    /// Байткод (SPIR-V).
    pub bytecode: Arc<[u8]>,
    /// Ключ байткода.
    pub key: u64,
    /// `true`, если отдан fallback-вариант.
    pub fallback: bool,
}

/// Библиотека деклараций, ядро fallback-ов и кэш пермутаций.
#[derive(Debug)]
pub struct ShaderLibrary {
    declarations: BTreeMap<u128, ShaderDeclaration>,
    core: BTreeMap<(u128, u32), Arc<[u8]>>,
    cache: PermutationCache,
    max_permutations: usize,
    stats: CacheStats,
}

impl ShaderLibrary {
    /// Библиотека с лимитом живых пермутаций (минимум 1).
    #[must_use]
    pub fn new(max_permutations: usize) -> Self {
        Self {
            declarations: BTreeMap::new(),
            core: BTreeMap::new(),
            cache: PermutationCache::new(),
            max_permutations: max_permutations.max(1),
            stats: CacheStats::default(),
        }
    }

    /// Регистрирует декларацию; UUID должен быть уникален.
    ///
    /// # Errors
    ///
    /// [`ShaderError::InvalidDeclaration`] — декларация невалидна или UUID
    /// уже занят.
    pub fn register(&mut self, declaration: ShaderDeclaration) -> Result<(), ShaderError> {
        declaration.validate()?;
        if self.declarations.contains_key(&declaration.shader_uuid) {
            return Err(ShaderError::InvalidDeclaration(
                "дубликат shader_uuid".to_owned(),
            ));
        }
        self.declarations
            .insert(declaration.shader_uuid, declaration);
        Ok(())
    }

    /// Декларация по UUID.
    #[must_use]
    pub fn declaration(&self, shader_uuid: u128) -> Option<&ShaderDeclaration> {
        self.declarations.get(&shader_uuid)
    }

    /// Число зарегистрированных шейдеров.
    #[must_use]
    pub fn len(&self) -> usize {
        self.declarations.len()
    }

    /// Пуста ли библиотека.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    /// Текущая статистика.
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    /// Кэш (чтение и запись PSO-уровня).
    #[must_use]
    pub fn cache(&self) -> &PermutationCache {
        &self.cache
    }

    /// Изменяемый кэш: рендер записывает сюда PSO после создания.
    pub fn cache_mut(&mut self) -> &mut PermutationCache {
        &mut self.cache
    }

    /// Число живых байткодов (ядро + on-demand).
    #[must_use]
    pub fn live_permutations(&self) -> usize {
        self.cache.bytecode_len()
    }

    /// Лимит живых пермутаций.
    #[must_use]
    pub fn max_permutations(&self) -> usize {
        self.max_permutations
    }

    /// Число прекомпилированных core-вариантов.
    #[must_use]
    pub fn core_len(&self) -> usize {
        self.core.len()
    }

    /// Прекомпилирует ядро: каждый pipeline-вариант без mesh/material keywords.
    ///
    /// # Errors
    ///
    /// [`ShaderError::CompileFailed`] — компилятор отверг вариант;
    /// [`ShaderError::PermutationBudgetExceeded`] — ядро не помещается в лимит.
    pub fn precompile_core(&mut self, compiler: &dyn ShaderCompiler) -> Result<usize, ShaderError> {
        let requests: Vec<(u128, u32)> = self
            .declarations
            .iter()
            .flat_map(|(uuid, declaration)| {
                (0..declaration.pipeline_variants.len()).map(move |index| (*uuid, index as u32))
            })
            .collect();
        if self.cache.bytecode_len() + requests.len() > self.max_permutations {
            return Err(ShaderError::PermutationBudgetExceeded {
                live: self.cache.bytecode_len() + requests.len(),
                max: self.max_permutations,
            });
        }

        let mut compiled = Vec::with_capacity(requests.len());
        for (shader_uuid, variant) in &requests {
            let declaration = self
                .declarations
                .get(shader_uuid)
                .ok_or(ShaderError::UnknownShader(*shader_uuid))?;
            let request = PermutationRequest::new(*shader_uuid, *variant);
            let bytecode = compiler.compile(declaration, &request)?;
            compiled.push((*shader_uuid, *variant, request.hash(), bytecode));
        }

        for (shader_uuid, variant, key, bytecode) in &compiled {
            let arc: Arc<[u8]> = Arc::from(bytecode.clone());
            self.core.insert((*shader_uuid, *variant), arc);
            if self.cache.insert_bytecode(*key, bytecode.clone()) {
                self.stats.compilations += 1;
            }
        }
        Ok(compiled.len())
    }

    /// Разрешает пермутацию: кэш → компиляция → fallback.
    ///
    /// # Errors
    ///
    /// [`ShaderError::UnknownShader`] — UUID не зарегистрирован;
    /// [`ShaderError::NoFallback`] — ядро ещё не прекомпилировано.
    pub fn resolve(
        &mut self,
        request: &PermutationRequest,
        compiler: &dyn ShaderCompiler,
    ) -> Result<ResolvedShader, ShaderError> {
        let declaration = self
            .declarations
            .get(&request.shader_uuid)
            .ok_or(ShaderError::UnknownShader(request.shader_uuid))?
            .clone();

        if request.pipeline_variant as usize >= declaration.pipeline_variants.len() {
            return self.fallback(&declaration, 0);
        }
        if !Self::mask_fits(&declaration, request.mesh, KeywordSet::Mesh)
            || !Self::mask_fits(&declaration, request.material, KeywordSet::Material)
        {
            return self.fallback(&declaration, request.pipeline_variant);
        }

        let key = request.hash();
        if let Some(bytecode) = self.cache.bytecode(key) {
            self.stats.hits += 1;
            return Ok(ResolvedShader {
                bytecode,
                key,
                fallback: false,
            });
        }

        self.stats.misses += 1;
        if self.cache.bytecode_len() >= self.max_permutations {
            self.stats.budget_exceeded += 1;
            return self.fallback(&declaration, request.pipeline_variant);
        }

        match compiler.compile(&declaration, request) {
            Ok(bytecode) => {
                self.stats.compilations += 1;
                let arc: Arc<[u8]> = Arc::from(bytecode.clone());
                self.cache.insert_bytecode(key, bytecode);
                self.cache
                    .insert_material(request.shader_uuid, request.material, key);
                Ok(ResolvedShader {
                    bytecode: arc,
                    key,
                    fallback: false,
                })
            }
            Err(_) => self.fallback(&declaration, request.pipeline_variant),
        }
    }

    fn mask_fits(declaration: &ShaderDeclaration, mask: KeywordMask, set: KeywordSet) -> bool {
        let count = declaration.keywords.len();
        if count < 64 && (mask.bits() >> count) != 0 {
            return false;
        }
        (0..count).all(|index| !mask.contains(index) || declaration.keywords[index].set == set)
    }

    fn fallback(
        &mut self,
        declaration: &ShaderDeclaration,
        variant: u32,
    ) -> Result<ResolvedShader, ShaderError> {
        self.stats.fallbacks += 1;
        let bytecode = self
            .core
            .get(&(declaration.shader_uuid, variant))
            .or_else(|| self.core.get(&(declaration.shader_uuid, 0)))
            .cloned()
            .ok_or(ShaderError::NoFallback(declaration.shader_uuid))?;
        let key = PermutationRequest::new(declaration.shader_uuid, variant).hash();
        Ok(ResolvedShader {
            bytecode,
            key,
            fallback: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shaders::{KeywordDecl, PipelineVariant, ShaderStage, VertexInput};

    struct FailingCompiler;

    impl ShaderCompiler for FailingCompiler {
        fn compile(
            &self,
            _declaration: &ShaderDeclaration,
            _request: &PermutationRequest,
        ) -> Result<Vec<u8>, ShaderError> {
            Err(ShaderError::CompileFailed("boom".to_owned()))
        }
    }

    fn declaration() -> ShaderDeclaration {
        use std::collections::BTreeMap;

        ShaderDeclaration {
            name: "pbr".to_owned(),
            shader_uuid: 42,
            stages: BTreeMap::from([
                (ShaderStage::Vertex, "pbr.vert.hlsl".to_owned()),
                (ShaderStage::Fragment, "pbr.frag.hlsl".to_owned()),
            ]),
            pipeline_variants: vec![
                PipelineVariant {
                    name: "deferred_gbuffer".to_owned(),
                    keywords: vec!["CLUSTERED_LIGHTING".to_owned()],
                },
                PipelineVariant {
                    name: "forward".to_owned(),
                    keywords: Vec::new(),
                },
            ],
            keywords: vec![
                KeywordDecl {
                    name: "CLUSTERED_LIGHTING".to_owned(),
                    set: KeywordSet::Pipeline,
                    default: true,
                },
                KeywordDecl {
                    name: "SKINNING".to_owned(),
                    set: KeywordSet::Mesh,
                    default: false,
                },
                KeywordDecl {
                    name: "NORMAL_MAP".to_owned(),
                    set: KeywordSet::Material,
                    default: false,
                },
            ],
            vertex_inputs: vec![VertexInput {
                location: 0,
                format: "R32G32B32_SFLOAT".to_owned(),
            }],
            bindings: Vec::new(),
        }
    }

    fn library(max: usize) -> ShaderLibrary {
        let mut library = ShaderLibrary::new(max);
        library.register(declaration()).expect("register");
        library
    }

    #[test]
    fn masks_follow_declaration_order() {
        let decl = declaration();
        let mask =
            KeywordMask::from_names(&decl, KeywordSet::Material, &["NORMAL_MAP"]).expect("mask");
        assert!(mask.contains(2));
        assert_eq!(mask.len(), 1);
        let mesh = KeywordMask::from_names(&decl, KeywordSet::Mesh, &["SKINNING"]).expect("mask");
        assert!(mask.union(mesh).intersects(mesh));
        assert!(!mask.intersects(mesh));
    }

    #[test]
    fn masks_reject_unknown_or_wrong_set() {
        let decl = declaration();
        assert!(matches!(
            KeywordMask::from_names(&decl, KeywordSet::Material, &["SKINNING"]),
            Err(ShaderError::UnknownKeyword(_))
        ));
        assert!(matches!(
            KeywordMask::from_names(&decl, KeywordSet::Mesh, &["NOPE"]),
            Err(ShaderError::UnknownKeyword(_))
        ));
    }

    #[test]
    fn request_hash_is_deterministic_and_distinct() {
        let base = PermutationRequest::new(42, 0);
        assert_eq!(base.hash(), PermutationRequest::new(42, 0).hash());
        let material = base.with_material(KeywordMask::from_bits(1 << 2));
        assert_ne!(base.hash(), material.hash());
        assert_ne!(
            PermutationRequest::new(42, 0).hash(),
            PermutationRequest::new(42, 1).hash()
        );
    }

    #[test]
    fn precompile_core_then_hits() {
        let mut library = library(8);
        let compiler = DeterministicCompiler;
        assert_eq!(library.precompile_core(&compiler).expect("core"), 2);
        assert_eq!(library.core_len(), 2);
        assert_eq!(library.live_permutations(), 2);

        let request = PermutationRequest::new(42, 0);
        let resolved = library.resolve(&request, &compiler).expect("resolve");
        assert!(!resolved.fallback);
        assert_eq!(library.stats().hits, 1);
    }

    #[test]
    fn unknown_variant_uses_fallback() {
        let mut library = library(8);
        let compiler = DeterministicCompiler;
        library.precompile_core(&compiler).expect("core");

        let request = PermutationRequest::new(42, 99);
        let resolved = library.resolve(&request, &compiler).expect("fallback");
        assert!(resolved.fallback);
        assert_eq!(library.stats().fallbacks, 1);
    }

    #[test]
    fn compiler_failure_uses_fallback() {
        let mut library = library(8);
        library
            .precompile_core(&DeterministicCompiler)
            .expect("core");

        let request = PermutationRequest::new(42, 0).with_material(
            KeywordMask::from_names(&declaration(), KeywordSet::Material, &["NORMAL_MAP"])
                .expect("mask"),
        );
        let resolved = library
            .resolve(&request, &FailingCompiler)
            .expect("fallback");
        assert!(resolved.fallback);
        assert_eq!(library.stats().compilations, 2);
    }

    #[test]
    fn budget_exceeded_uses_fallback() {
        let mut library = library(3);
        let compiler = DeterministicCompiler;
        library.precompile_core(&compiler).expect("core");

        let decl = declaration();
        let mesh = KeywordMask::from_names(&decl, KeywordSet::Mesh, &["SKINNING"]).expect("mask");
        let material =
            KeywordMask::from_names(&decl, KeywordSet::Material, &["NORMAL_MAP"]).expect("mask");

        let first = PermutationRequest::new(42, 0).with_mesh(mesh);
        assert!(
            !library
                .resolve(&first, &compiler)
                .expect("compile")
                .fallback
        );
        assert_eq!(library.live_permutations(), 3);

        let second = PermutationRequest::new(42, 0).with_material(material);
        let resolved = library.resolve(&second, &compiler).expect("fallback");
        assert!(resolved.fallback);
        assert_eq!(library.stats().budget_exceeded, 1);
    }

    #[test]
    fn material_level_cache_is_filled() {
        let mut library = library(8);
        let compiler = DeterministicCompiler;
        library.precompile_core(&compiler).expect("core");

        let decl = declaration();
        let material =
            KeywordMask::from_names(&decl, KeywordSet::Material, &["NORMAL_MAP"]).expect("mask");
        let request = PermutationRequest::new(42, 0).with_material(material);
        let resolved = library.resolve(&request, &compiler).expect("compile");
        assert_eq!(
            library.cache().material_entry(42, material),
            Some(resolved.key)
        );
    }

    #[test]
    fn invalid_masks_fall_back() {
        let mut library = library(8);
        let compiler = DeterministicCompiler;
        library.precompile_core(&compiler).expect("core");

        // Бит mesh-маски указывает на material-keyword.
        let request = PermutationRequest::new(42, 0).with_mesh(KeywordMask::from_bits(1 << 2));
        let resolved = library.resolve(&request, &compiler).expect("fallback");
        assert!(resolved.fallback);
    }

    #[test]
    fn libraries_are_deterministic() {
        let compiler = DeterministicCompiler;
        let mut first = library(8);
        let mut second = library(8);
        first.precompile_core(&compiler).expect("core");
        second.precompile_core(&compiler).expect("core");

        let request = PermutationRequest::new(42, 1).with_material(KeywordMask::from_bits(1 << 2));
        let a = first.resolve(&request, &compiler).expect("resolve");
        let b = second.resolve(&request, &compiler).expect("resolve");
        assert_eq!(a.key, b.key);
        assert_eq!(a.bytecode, b.bytecode);
        assert_eq!(a.fallback, b.fallback);
    }

    #[test]
    fn pso_level_is_separate() {
        let mut cache = PermutationCache::new();
        assert!(cache.pso(7).is_none());
        cache.insert_pso(7, PipelineId(3));
        assert_eq!(cache.pso(7), Some(PipelineId(3)));
        assert_eq!(cache.pso_len(), 1);
        cache.clear();
        assert_eq!(cache.bytecode_len(), 0);
        assert_eq!(cache.pso_len(), 0);
    }
}
