//! Декларация шейдера: `shader.json` рядом с исходником (ADR-0014).
//!
//! Декларация — ABI контента: stages, pipeline-варианты, три набора keywords,
//! vertex inputs и биндинги. Валидация выполняется на этапе загрузки, чтобы
//! ошибка конфигурации была типизированной, а не чёрным экраном в проде.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::ShaderError;

/// Стадия шейдера.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderStage {
    /// Вершинная стадия.
    Vertex,
    /// Фрагментная стадия.
    Fragment,
    /// Compute-стадия.
    Compute,
}

/// Набор keywords (три источника, ADR-0014).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeywordSet {
    /// Render Graph / FeatureSet: `CLUSTERED_LIGHTING`, `SHADOWS_CSM`.
    Pipeline,
    /// Меш: `SKINNING`, `MORPH_TARGET`.
    Mesh,
    /// Материал: `NORMAL_MAP`, `PARALLAX`, `EMISSIVE`.
    Material,
}

/// Объявление keyword.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeywordDecl {
    /// Имя keyword (уникально в декларации).
    pub name: String,
    /// Набор-источник.
    pub set: KeywordSet,
    /// Входит ли keyword в ядро (прекомпилируется всегда).
    #[serde(default)]
    pub default: bool,
}

/// Функциональный вариант пайплайна.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineVariant {
    /// Имя варианта (`deferred_gbuffer`, `forward`, `deferred_lighting`).
    pub name: String,
    /// Pipeline-keywords, включаемые вариантом.
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// Вершинный вход (layout v1, ADR-0035).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VertexInput {
    /// Индекс location.
    pub location: u32,
    /// Формат (`R32G32B32_SFLOAT` в layout v1).
    pub format: String,
}

/// Биндинг ресурса; проверяется на этапе компиляции.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingDecl {
    /// Имя биндинга.
    pub name: String,
    /// Descriptor set.
    pub group: u32,
    /// Binding внутри set.
    pub binding: u32,
}

/// Максимум keywords на шейдер: маска — `u64`.
pub const MAX_KEYWORDS: usize = 64;
/// Максимум material-keywords (бюджет пермутаций, ADR-0014).
pub const MAX_MATERIAL_KEYWORDS: usize = 6;
/// Единственный формат вершинного входа layout v1.
pub const VERTEX_FORMAT_V1: &str = "R32G32B32_SFLOAT";

/// Декларация шейдера.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShaderDeclaration {
    /// Имя шейдера.
    pub name: String,
    /// Стабильный UUID базового шейдера.
    pub shader_uuid: u128,
    /// Исходники по стадиям.
    pub stages: BTreeMap<ShaderStage, String>,
    /// Функциональные варианты пайплайна.
    pub pipeline_variants: Vec<PipelineVariant>,
    /// Объявленные keywords (порядок = порядок битов маски).
    pub keywords: Vec<KeywordDecl>,
    /// Вершинные входы.
    pub vertex_inputs: Vec<VertexInput>,
    /// Биндинги ресурсов.
    pub bindings: Vec<BindingDecl>,
}

impl ShaderDeclaration {
    /// Разбирает `shader.json` и валидирует декларацию.
    ///
    /// # Errors
    ///
    /// [`ShaderError::InvalidDeclaration`] — битый JSON или нарушение
    /// инвариантов (дубликаты, бюджет keywords, неизвестный формат).
    pub fn from_json(json: &str) -> Result<Self, ShaderError> {
        let declaration: Self = serde_json::from_str(json)
            .map_err(|error| ShaderError::InvalidDeclaration(error.to_string()))?;
        declaration.validate()?;
        Ok(declaration)
    }

    /// Сериализует декларацию в JSON.
    ///
    /// # Errors
    ///
    /// [`ShaderError::InvalidDeclaration`] — ошибка сериализации.
    pub fn to_json(&self) -> Result<String, ShaderError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| ShaderError::InvalidDeclaration(error.to_string()))
    }

    /// Проверяет инварианты декларации.
    ///
    /// # Errors
    ///
    /// [`ShaderError::InvalidDeclaration`] с причиной отказа.
    pub fn validate(&self) -> Result<(), ShaderError> {
        let invalid = |reason: &str| ShaderError::InvalidDeclaration(reason.to_owned());
        if self.name.is_empty() {
            return Err(invalid("пустое имя шейдера"));
        }
        if self.shader_uuid == 0 {
            return Err(invalid("нулевой shader_uuid"));
        }
        if self.stages.is_empty() || self.stages.values().any(String::is_empty) {
            return Err(invalid("не заданы исходники стадий"));
        }
        if !self.stages.contains_key(&ShaderStage::Vertex) {
            return Err(invalid("нет вершинной стадии"));
        }
        if !self.stages.contains_key(&ShaderStage::Fragment)
            && !self.stages.contains_key(&ShaderStage::Compute)
        {
            return Err(invalid("нет фрагментной или compute-стадии"));
        }
        if self.pipeline_variants.is_empty() {
            return Err(invalid("нет pipeline-вариантов"));
        }
        if self.keywords.len() > MAX_KEYWORDS {
            return Err(invalid("слишком много keywords (> 64)"));
        }

        let mut names = BTreeSet::new();
        let mut material_keywords = 0_usize;
        for keyword in &self.keywords {
            if keyword.name.is_empty() {
                return Err(invalid("пустое имя keyword"));
            }
            if !names.insert(keyword.name.as_str()) {
                return Err(invalid("дубликат keyword"));
            }
            if keyword.set == KeywordSet::Material {
                material_keywords += 1;
            }
        }
        if material_keywords > MAX_MATERIAL_KEYWORDS {
            return Err(invalid("больше 6 material-keywords"));
        }

        let pipeline_keywords: BTreeSet<&str> = self
            .keywords
            .iter()
            .filter(|keyword| keyword.set == KeywordSet::Pipeline)
            .map(|keyword| keyword.name.as_str())
            .collect();
        let mut variants = BTreeSet::new();
        for variant in &self.pipeline_variants {
            if variant.name.is_empty() {
                return Err(invalid("пустое имя pipeline-варианта"));
            }
            if !variants.insert(variant.name.as_str()) {
                return Err(invalid("дубликат pipeline-варианта"));
            }
            for keyword in &variant.keywords {
                if !pipeline_keywords.contains(keyword.as_str()) {
                    return Err(invalid("вариант ссылается на неизвестный pipeline-keyword"));
                }
            }
        }

        let mut locations = BTreeSet::new();
        for input in &self.vertex_inputs {
            if input.format != VERTEX_FORMAT_V1 {
                return Err(invalid("формат вершины не из layout v1"));
            }
            if !locations.insert(input.location) {
                return Err(invalid("дубликат vertex location"));
            }
        }

        let mut slots = BTreeSet::new();
        for binding in &self.bindings {
            if !slots.insert((binding.group, binding.binding)) {
                return Err(invalid("дубликат биндинга"));
            }
        }
        Ok(())
    }

    /// Индекс keyword в порядке декларации (он же бит маски).
    #[must_use]
    pub fn keyword_index(&self, name: &str) -> Option<usize> {
        self.keywords
            .iter()
            .position(|keyword| keyword.name == name)
    }

    /// Keywords указанного набора в порядке декларации.
    pub fn keywords_of(&self, set: KeywordSet) -> impl Iterator<Item = &KeywordDecl> {
        self.keywords
            .iter()
            .filter(move |keyword| keyword.set == set)
    }

    /// Число material-keywords (бюджет пермутаций).
    #[must_use]
    pub fn material_keyword_count(&self) -> usize {
        self.keywords_of(KeywordSet::Material).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> ShaderDeclaration {
        ShaderDeclaration {
            name: "pbr".to_owned(),
            shader_uuid: 42,
            stages: BTreeMap::from([
                (ShaderStage::Vertex, "pbr.vert.hlsl".to_owned()),
                (ShaderStage::Fragment, "pbr.frag.hlsl".to_owned()),
            ]),
            pipeline_variants: vec![PipelineVariant {
                name: "deferred_gbuffer".to_owned(),
                keywords: vec!["CLUSTERED_LIGHTING".to_owned()],
            }],
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
                format: VERTEX_FORMAT_V1.to_owned(),
            }],
            bindings: vec![BindingDecl {
                name: "albedo".to_owned(),
                group: 0,
                binding: 0,
            }],
        }
    }

    #[test]
    fn valid_declaration_passes() {
        assert!(declaration().validate().is_ok());
        assert_eq!(declaration().material_keyword_count(), 1);
    }

    #[test]
    fn duplicate_keywords_are_rejected() {
        let mut decl = declaration();
        decl.keywords.push(decl.keywords[0].clone());
        assert!(matches!(
            decl.validate(),
            Err(ShaderError::InvalidDeclaration(_))
        ));
    }

    #[test]
    fn material_keyword_budget_is_enforced() {
        let mut decl = declaration();
        for index in 0..MAX_MATERIAL_KEYWORDS {
            decl.keywords.push(KeywordDecl {
                name: format!("MATERIAL_{index}"),
                set: KeywordSet::Material,
                default: false,
            });
        }
        assert!(decl.validate().is_err());
    }

    #[test]
    fn variant_must_reference_pipeline_keyword() {
        let mut decl = declaration();
        decl.pipeline_variants[0].keywords = vec!["SKINNING".to_owned()];
        assert!(decl.validate().is_err());
    }

    #[test]
    fn duplicate_bindings_are_rejected() {
        let mut decl = declaration();
        decl.bindings.push(decl.bindings[0].clone());
        assert!(decl.validate().is_err());
    }

    #[test]
    fn vertex_format_is_layout_v1() {
        let mut decl = declaration();
        decl.vertex_inputs[0].format = "R32G32_SFLOAT".to_owned();
        assert!(decl.validate().is_err());
    }

    #[test]
    fn json_roundtrip_is_stable() {
        let json = r#"{
            "name": "pbr",
            "shader_uuid": 42,
            "stages": { "vertex": "pbr.vert.hlsl", "fragment": "pbr.frag.hlsl" },
            "pipeline_variants": [
                { "name": "deferred_gbuffer", "keywords": ["CLUSTERED_LIGHTING"] }
            ],
            "keywords": [
                { "name": "CLUSTERED_LIGHTING", "set": "pipeline", "default": true },
                { "name": "SKINNING", "set": "mesh" },
                { "name": "NORMAL_MAP", "set": "material" }
            ],
            "vertex_inputs": [ { "location": 0, "format": "R32G32B32_SFLOAT" } ],
            "bindings": [ { "name": "albedo", "group": 0, "binding": 0 } ]
        }"#;
        let decl = ShaderDeclaration::from_json(json).expect("valid declaration");
        let again = ShaderDeclaration::from_json(&decl.to_json().expect("json"))
            .expect("roundtrip declaration");
        assert_eq!(decl, again);
        assert_eq!(decl, declaration());
    }

    #[test]
    fn broken_json_is_a_typed_error() {
        assert!(matches!(
            ShaderDeclaration::from_json("{"),
            Err(ShaderError::InvalidDeclaration(_))
        ));
    }
}
