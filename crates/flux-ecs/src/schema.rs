//! `SchemaRegistry`: схемы компонентов как данные (ADR-0030).
//!
//! Один источник правды для сейвов, wire-схемы, undo и инспектора.
//! Поля отсортированы по `field_uuid`; реестр — по `(type_uuid, schema_rev)`,
//! поэтому обход детерминирован и не зависит от `HashMap`.
//!
//! Дерайв `Reflect` (`flux-macros`) заполняет [`ComponentSchema`]:
//! `#[save]`, `#[replicated]`, `#[field(uuid = "...")]`,
//! `#[reflect(min, max, slider, category, tooltip, visible)]`.

use std::collections::BTreeMap;

use flux_core::schema::{FieldFlags, FieldId, SchemaRef, TypeId};
use thiserror::Error;

/// Тип значения поля (закрытое перечисление, ADR-0030).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FieldType {
    /// `f32`.
    F32,
    /// `f64`.
    F64,
    /// `i32`.
    I32,
    /// `u32`.
    U32,
    /// `i64`.
    I64,
    /// `u64`.
    U64,
    /// `bool`.
    Bool,
    /// `String`.
    String,
    /// `[f32; 3]`.
    Vec3,
    /// `[f64; 3]`.
    DVec3,
    /// `WorldPos` (i64-нанометры).
    WorldPos,
    /// `Uuid`.
    Uuid,
    /// Enum: хранится номер варианта.
    Enum,
    /// Сырые байты.
    Bytes,
    /// Тип, для которого у дерайва нет маппинга (инспектор покажет как opaque).
    Other,
}

/// Метаданные инспектора.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EditorMeta {
    /// Минимум для слайдера/спиннера.
    pub min: Option<f64>,
    /// Максимум для слайдера/спиннера.
    pub max: Option<f64>,
    /// Показывать слайдер вместо числа.
    pub slider: bool,
    /// Категория в инспекторе.
    pub category: Option<String>,
    /// Подсказка.
    pub tooltip: Option<String>,
    /// Видимость в инспекторе.
    pub visible: bool,
}

impl EditorMeta {
    /// Видимое поле без ограничений.
    #[must_use]
    pub fn visible() -> Self {
        Self {
            visible: true,
            ..Self::default()
        }
    }
}

/// Политика обработки осиротевших записей компонента.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum OrphanPolicy {
    /// Запись пропускается, факт пишется в лог (по умолчанию).
    #[default]
    Log,
    /// Сущность пересоздаётся.
    Respawn,
    /// Запись отбрасывается без восстановления.
    Drop,
}

/// Схема поля компонента.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FieldSchema {
    /// Стабильный идентификатор поля (совместимость, не имя).
    pub field_uuid: FieldId,
    /// Имя поля в коде.
    pub name: &'static str,
    /// Тип значения.
    pub ty: FieldType,
    /// Флаги сейва/репликации.
    pub flags: FieldFlags,
    /// Метаданные инспектора.
    pub editor: EditorMeta,
    /// RON значения по умолчанию (для инспектора).
    pub default_ron: Option<String>,
}

impl FieldSchema {
    /// Поле с флагами и типом; метаданные — видимые по умолчанию.
    #[must_use]
    pub fn new(field_uuid: FieldId, name: &'static str, ty: FieldType, flags: FieldFlags) -> Self {
        Self {
            field_uuid,
            name,
            ty,
            flags,
            editor: EditorMeta::visible(),
            default_ron: None,
        }
    }

    /// Задаёт метаданные инспектора.
    #[must_use]
    pub fn with_editor(mut self, editor: EditorMeta) -> Self {
        self.editor = editor;
        self
    }
}

/// Схема компонента как данные.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ComponentSchema {
    /// Тип и ревизия.
    pub schema: SchemaRef,
    /// Имя типа в коде.
    pub name: &'static str,
    /// Поля (отсортированы по `field_uuid`).
    pub fields: Vec<FieldSchema>,
    /// Политика осиротевших записей.
    pub orphan_policy: OrphanPolicy,
}

impl ComponentSchema {
    /// Пустая схема типа.
    #[must_use]
    pub fn new(schema: SchemaRef, name: &'static str) -> Self {
        Self {
            schema,
            name,
            fields: Vec::new(),
            orphan_policy: OrphanPolicy::default(),
        }
    }

    /// Добавляет поле.
    #[must_use]
    pub fn with_field(mut self, field: FieldSchema) -> Self {
        self.fields.push(field);
        self
    }

    /// Поле по UUID.
    #[must_use]
    pub fn field(&self, field_uuid: FieldId) -> Option<&FieldSchema> {
        self.fields
            .iter()
            .find(|field| field.field_uuid == field_uuid)
    }

    /// Поля, попадающие в сейв.
    pub fn save_fields(&self) -> impl Iterator<Item = &FieldSchema> + '_ {
        self.fields.iter().filter(|field| field.flags.save)
    }

    /// Поля, попадающие в wire-схему.
    pub fn replicated_fields(&self) -> impl Iterator<Item = &FieldSchema> + '_ {
        self.fields.iter().filter(|field| field.flags.replicated)
    }

    /// Сортирует поля по `field_uuid` (канонический порядок).
    pub fn sort_fields(&mut self) {
        self.fields.sort_by_key(|field| field.field_uuid);
    }

    /// Проверяет уникальность UUID полей.
    ///
    /// # Errors
    ///
    /// [`SchemaError::DuplicateField`].
    pub fn validate(&self) -> Result<(), SchemaError> {
        let mut seen = Vec::with_capacity(self.fields.len());
        for field in &self.fields {
            if seen.contains(&field.field_uuid) {
                return Err(SchemaError::DuplicateField(field.field_uuid));
            }
            seen.push(field.field_uuid);
        }
        Ok(())
    }
}

/// Ошибка реестра схем.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SchemaError {
    /// `(type_uuid, schema_rev)` зарегистрирован с другим содержимым.
    #[error("schema revision conflict: {schema:?}")]
    RevisionConflict {
        /// Конфликтующая ссылка.
        schema: SchemaRef,
    },
    /// Два поля имеют одинаковый UUID.
    #[error("duplicate field uuid: {0:?}")]
    DuplicateField(FieldId),
}

/// Реестр схем: детерминированный обход по `(type_uuid, schema_rev)`.
#[derive(Default)]
pub struct SchemaRegistry {
    by_ref: BTreeMap<SchemaRef, ComponentSchema>,
    by_type: BTreeMap<TypeId, SchemaRef>,
}

impl SchemaRegistry {
    /// Пустой реестр.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует схему (идемпотентно при совпадении содержимого).
    ///
    /// # Errors
    ///
    /// [`SchemaError::RevisionConflict`] — та же ревизия с другим содержимым;
    /// [`SchemaError::DuplicateField`] — повтор UUID поля.
    pub fn register(&mut self, mut schema: ComponentSchema) -> Result<(), SchemaError> {
        schema.sort_fields();
        schema.validate()?;

        if let Some(existing) = self.by_ref.get(&schema.schema) {
            if existing == &schema {
                return Ok(());
            }
            return Err(SchemaError::RevisionConflict {
                schema: schema.schema,
            });
        }

        let reference = schema.schema;
        self.by_type
            .entry(reference.type_uuid)
            .and_modify(|current| {
                if reference.schema_rev > current.schema_rev {
                    *current = reference;
                }
            })
            .or_insert(reference);
        self.by_ref.insert(reference, schema);
        Ok(())
    }

    /// Текущая (последняя) схема типа.
    #[must_use]
    pub fn get(&self, type_uuid: TypeId) -> Option<&ComponentSchema> {
        let reference = self.by_type.get(&type_uuid)?;
        self.by_ref.get(reference)
    }

    /// Схема конкретной ревизии.
    #[must_use]
    pub fn get_ref(&self, schema: SchemaRef) -> Option<&ComponentSchema> {
        self.by_ref.get(&schema)
    }

    /// Поле конкретной ревизии.
    #[must_use]
    pub fn field(&self, schema: SchemaRef, field_uuid: FieldId) -> Option<&FieldSchema> {
        self.get_ref(schema)?.field(field_uuid)
    }

    /// Число зарегистрированных ревизий.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_ref.len()
    }

    /// Пуст ли реестр.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_ref.is_empty()
    }

    /// Обход ревизий в порядке `(type_uuid, schema_rev)`.
    pub fn schemas(&self) -> impl Iterator<Item = &ComponentSchema> + '_ {
        self.by_ref.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn type_id(seed: u128) -> TypeId {
        TypeId::from_uuid(Uuid::from_u128(seed))
    }

    fn field(seed: u128, name: &'static str) -> FieldSchema {
        FieldSchema::new(
            FieldId(Uuid::from_u128(seed)),
            name,
            FieldType::F32,
            FieldFlags::saved(),
        )
    }

    fn schema(rev: u32) -> ComponentSchema {
        ComponentSchema::new(
            SchemaRef::new(type_id(0x0000_0000_0000_4000_8000_0000_0000_0100), rev),
            "Position",
        )
        .with_field(field(0x0000_0000_0000_4000_8000_0000_0000_0102, "x"))
        .with_field(field(0x0000_0000_0000_4000_8000_0000_0000_0101, "y"))
    }

    #[test]
    fn registry_sorts_fields_and_is_idempotent() {
        let mut registry = SchemaRegistry::new();
        registry.register(schema(1)).unwrap();
        registry.register(schema(1)).unwrap();
        assert_eq!(registry.len(), 1);

        let stored = registry
            .get(type_id(0x0000_0000_0000_4000_8000_0000_0000_0100))
            .unwrap();
        let names: Vec<&str> = stored.fields.iter().map(|field| field.name).collect();
        assert_eq!(names, vec!["y", "x"]);
    }

    #[test]
    fn revision_conflict_is_rejected() {
        let mut registry = SchemaRegistry::new();
        registry.register(schema(1)).unwrap();

        let mut changed = schema(1);
        changed.fields[0].ty = FieldType::F64;
        assert_eq!(
            registry.register(changed),
            Err(SchemaError::RevisionConflict {
                schema: schema(1).schema
            })
        );
    }

    #[test]
    fn new_revision_becomes_current() {
        let mut registry = SchemaRegistry::new();
        registry.register(schema(1)).unwrap();
        registry.register(schema(2)).unwrap();

        let type_uuid = type_id(0x0000_0000_0000_4000_8000_0000_0000_0100);
        assert_eq!(registry.get(type_uuid).unwrap().schema.schema_rev, 2);
        assert!(registry.get_ref(SchemaRef::new(type_uuid, 1)).is_some());
    }

    #[test]
    fn duplicate_field_uuid_is_rejected() {
        let mut registry = SchemaRegistry::new();
        let mut duplicate = ComponentSchema::new(schema(1).schema, "Broken");
        duplicate.fields.push(field(7, "a"));
        duplicate.fields.push(field(7, "b"));
        assert_eq!(
            registry.register(duplicate),
            Err(SchemaError::DuplicateField(FieldId(Uuid::from_u128(7))))
        );
    }
}
