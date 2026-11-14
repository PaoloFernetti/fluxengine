//! Заделы схем: идентификаторы типов/полей и версия схемы на записи.
//!
//! Полный `SchemaRegistry` живёт в `flux-ecs` (этап 2); здесь — типы, которые
//! обязаны появиться до первых данных (сейвы, сеть, инспектор).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `const`-конструктор UUID из `u128` для сгенерированного кода дерайвов.
#[must_use]
pub const fn uuid_from_u128(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

/// Идентификатор типа компонента.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TypeId(pub Uuid);

impl TypeId {
    /// Новый случайный идентификатор.
    #[must_use]
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Из существующего UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for TypeId {
    fn default() -> Self {
        Self::new_v4()
    }
}

/// Идентификатор поля компонента: стабилен на всю жизнь типа.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FieldId(pub Uuid);

impl FieldId {
    /// Новый идентификатор поля.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for FieldId {
    fn default() -> Self {
        Self::new()
    }
}

/// Версия схемы типа на каждой записи (диск и провод).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaRef {
    /// Тип компонента.
    pub type_uuid: TypeId,
    /// Ревизия схемы типа (не версия игры).
    pub schema_rev: u32,
}

impl SchemaRef {
    /// Создаёт ссылку на ревизию схемы.
    #[must_use]
    pub const fn new(type_uuid: TypeId, schema_rev: u32) -> Self {
        Self {
            type_uuid,
            schema_rev,
        }
    }

    /// Следующая ревизия того же типа.
    #[must_use]
    pub fn bumped(self) -> Self {
        Self {
            type_uuid: self.type_uuid,
            schema_rev: self.schema_rev.saturating_add(1),
        }
    }
}

/// Разметка поля: участвует ли оно в сейве и в сетевой репликации.
///
/// Заготовка маркеров `#[save]` / `#[replicated]`: атрибуты появятся вместе с
/// `#[derive(Reflect)]` в `flux-ecs` (этап 2), а компилируются они ровно в эти
/// флаги. Формат фиксируется сейчас, до первых сейвов и первых wire-схем.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldFlags {
    /// Поле пишется в `SaveState`.
    pub save: bool,
    /// Поле входит в wire-схему репликации.
    pub replicated: bool,
}

impl FieldFlags {
    /// Обычное поле: ни сейва, ни репликации.
    #[must_use]
    pub const fn transient() -> Self {
        Self {
            save: false,
            replicated: false,
        }
    }

    /// Поле сохраняется, но не реплицируется.
    #[must_use]
    pub const fn saved() -> Self {
        Self {
            save: true,
            replicated: false,
        }
    }

    /// Поле реплицируется (и по умолчанию сохраняется).
    #[must_use]
    pub const fn replicated() -> Self {
        Self {
            save: true,
            replicated: true,
        }
    }
}

/// Поле компонента с разметкой.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDescriptor {
    /// Стабильный идентификатор поля.
    pub id: FieldId,
    /// Флаги сейва/репликации.
    pub flags: FieldFlags,
}

/// Схема компонента как данные: ревизия плюс разметка полей.
///
/// Заготовка `SchemaRegistry` (полный реестр — `flux-ecs`, этап 2). Уже сейчас
/// позволяет зафиксировать ABI сейвов и wire-схем до появления данных.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentLayout {
    /// Тип и ревизия схемы.
    pub schema: SchemaRef,
    /// Поля в порядке объявления.
    pub fields: Vec<FieldDescriptor>,
}

impl ComponentLayout {
    /// Создаёт схему компонента.
    #[must_use]
    pub fn new(schema: SchemaRef) -> Self {
        Self {
            schema,
            fields: Vec::new(),
        }
    }

    /// Добавляет поле с разметкой.
    pub fn with_field(mut self, id: FieldId, flags: FieldFlags) -> Self {
        self.fields.push(FieldDescriptor { id, flags });
        self
    }

    /// Поля, попадающие в сейв.
    pub fn save_fields(&self) -> impl Iterator<Item = FieldId> + '_ {
        self.fields
            .iter()
            .filter(|field| field.flags.save)
            .map(|field| field.id)
    }

    /// Поля, попадающие в wire-схему.
    pub fn replicated_fields(&self) -> impl Iterator<Item = FieldId> + '_ {
        self.fields
            .iter()
            .filter(|field| field.flags.replicated)
            .map(|field| field.id)
    }

    /// Совместимы ли схемы по ревизии.
    #[must_use]
    pub fn same_revision(&self, other: &Self) -> bool {
        self.schema == other.schema
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_ref_roundtrips_through_ron() {
        let schema = SchemaRef::new(TypeId::new_v4(), 3);
        let text = ron::to_string(&schema).unwrap();
        let back: SchemaRef = ron::from_str(&text).unwrap();
        assert_eq!(schema, back);
    }

    #[test]
    fn bump_increments_revision_only() {
        let type_id = TypeId::new_v4();
        let schema = SchemaRef::new(type_id, 1);
        let next = schema.bumped();
        assert_eq!(next.type_uuid, type_id);
        assert_eq!(next.schema_rev, 2);
    }

    #[test]
    fn field_ids_are_unique() {
        assert_ne!(FieldId::new(), FieldId::new());
    }

    #[test]
    fn component_layout_roundtrips_through_ron() {
        let layout = ComponentLayout::new(SchemaRef::new(TypeId::new_v4(), 2))
            .with_field(FieldId::new(), FieldFlags::saved())
            .with_field(FieldId::new(), FieldFlags::replicated())
            .with_field(FieldId::new(), FieldFlags::transient());

        let text = ron::to_string(&layout).unwrap();
        let back: ComponentLayout = ron::from_str(&text).unwrap();

        assert_eq!(layout, back);
        assert_eq!(layout.save_fields().count(), 2);
        assert_eq!(layout.replicated_fields().count(), 1);
    }

    #[test]
    fn revision_bump_is_detected() {
        let layout = ComponentLayout::new(SchemaRef::new(TypeId::new_v4(), 1));
        let bumped = ComponentLayout::new(layout.schema.bumped());

        assert!(layout.same_revision(&layout));
        assert!(!layout.same_revision(&bumped));
    }
}
