//! `ComponentDelta` и append-only `SaveJournal` (ADR-0030).
//!
//! Один формат для сейва, wire-схемы и undo: `entity_uuid + SchemaRef +
//! поля по field_uuid`. Порядок канонический — дельты по
//! `(entity_uuid, type_uuid, schema_rev)`, поля по `field_uuid`; одинаковый
//! вход даёт байт-в-байт одинаковый RON.
//!
//! Неизвестное поле указанной ревизии не теряется молча: [`SaveJournal::sanitize`]
//! убирает его и пишет [`OrphanedField`] в лог журнала.

use flux_core::schema::{FieldId, SchemaRef, TypeId};
use flux_core::time::SimTick;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::EntityUuid;
use crate::schema::SchemaRegistry;

/// Значение поля в дельте (закрытое перечисление, ADR-0030).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// `f32`.
    F32(f32),
    /// `f64`.
    F64(f64),
    /// `i32`.
    I32(i32),
    /// `u32`.
    U32(u32),
    /// `i64`.
    I64(i64),
    /// `u64`.
    U64(u64),
    /// `bool`.
    Bool(bool),
    /// `String`.
    String(String),
    /// `[f32; 3]`.
    Vec3([f32; 3]),
    /// `[f64; 3]`.
    DVec3([f64; 3]),
    /// `WorldPos` в нанометрах.
    WorldPos([i64; 3]),
    /// UUID (ссылки, идентификаторы).
    Uuid(Uuid),
    /// Enum: номер варианта.
    Enum {
        /// Номер варианта.
        variant: u32,
    },
    /// Сырые байты.
    Bytes(Vec<u8>),
}

/// Поле дельты: идентификатор совместимости и значение.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldValue {
    /// Стабильный UUID поля.
    pub field: FieldId,
    /// Значение.
    pub value: Value,
}

impl FieldValue {
    /// Пара «поле — значение».
    #[must_use]
    pub fn new(field: FieldId, value: Value) -> Self {
        Self { field, value }
    }
}

/// Дельта компонента: частичная по построению.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentDelta {
    /// Стабильный UUID сущности.
    pub entity: EntityUuid,
    /// Тип и ревизия схемы.
    pub schema: SchemaRef,
    /// Поля по `field_uuid`.
    pub fields: Vec<FieldValue>,
}

impl ComponentDelta {
    /// Пустая дельта компонента.
    #[must_use]
    pub fn new(entity: EntityUuid, schema: SchemaRef) -> Self {
        Self {
            entity,
            schema,
            fields: Vec::new(),
        }
    }

    /// Добавляет поле.
    #[must_use]
    pub fn with_field(mut self, field: FieldValue) -> Self {
        self.fields.push(field);
        self
    }

    /// Значение поля.
    #[must_use]
    pub fn field(&self, field: FieldId) -> Option<&Value> {
        self.fields
            .iter()
            .find(|value| value.field == field)
            .map(|value| &value.value)
    }

    /// Канонический порядок полей.
    pub fn sort(&mut self) {
        self.fields.sort_by_key(|field| field.field);
    }

    /// Ключ сортировки дельт.
    fn order_key(&self) -> (EntityUuid, TypeId, u32) {
        (self.entity, self.schema.type_uuid, self.schema.schema_rev)
    }
}

/// Осиротевшее поле: нет `field_uuid` в указанной ревизии.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrphanedField {
    /// Сущность.
    pub entity: EntityUuid,
    /// Схема, в которой поля нет.
    pub schema: SchemaRef,
    /// Поле.
    pub field: FieldId,
}

/// Запись журнала: момент снятия и дельты.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Монотонный номер записи.
    pub seq: u64,
    /// Тик снятия.
    pub tick: SimTick,
    /// Дельты в каноническом порядке.
    pub deltas: Vec<ComponentDelta>,
}

/// Append-only журнал дельт.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SaveJournal {
    /// Последний выданный `seq`.
    pub seq: u64,
    /// Записи в порядке добавления.
    pub entries: Vec<JournalEntry>,
    /// Осиротевшие поля (явный лог, не молчаливая потеря).
    pub orphans: Vec<OrphanedField>,
}

impl SaveJournal {
    /// Пустой журнал.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Число записей.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Пуст ли журнал.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Последний выданный `seq`.
    #[must_use]
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Осиротевшие поля.
    #[must_use]
    pub fn orphans(&self) -> &[OrphanedField] {
        &self.orphans
    }

    /// Добавляет запись; дельты и поля приводятся к каноническому порядку.
    pub fn append(&mut self, tick: SimTick, mut deltas: Vec<ComponentDelta>) -> u64 {
        for delta in &mut deltas {
            delta.sort();
        }
        deltas.sort_by_key(ComponentDelta::order_key);
        self.seq += 1;
        self.entries.push(JournalEntry {
            seq: self.seq,
            tick,
            deltas,
        });
        self.seq
    }

    /// Убирает поля, отсутствующие в реестре, и пишет их в `orphans`.
    ///
    /// Возвращает число удалённых полей.
    pub fn sanitize(&mut self, registry: &SchemaRegistry) -> usize {
        let mut removed = 0;
        let mut orphans = Vec::new();
        for entry in &mut self.entries {
            for delta in &mut entry.deltas {
                let before = delta.fields.len();
                let mut kept = Vec::with_capacity(before);
                for field in delta.fields.drain(..) {
                    if registry.field(delta.schema, field.field).is_some() {
                        kept.push(field);
                    } else {
                        orphans.push(OrphanedField {
                            entity: delta.entity,
                            schema: delta.schema,
                            field: field.field,
                        });
                    }
                }
                delta.fields = kept;
                removed += before - delta.fields.len();
            }
        }
        self.orphans.extend(orphans);
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ComponentSchema, FieldSchema, FieldType, SchemaRegistry};
    use flux_core::schema::{FieldFlags, TypeId};
    use uuid::Uuid;

    fn type_id(seed: u128) -> TypeId {
        TypeId::from_uuid(Uuid::from_u128(seed))
    }

    fn field_id(seed: u128) -> FieldId {
        FieldId(Uuid::from_u128(seed))
    }

    fn schema_ref() -> SchemaRef {
        SchemaRef::new(type_id(0x0000_0000_0000_4000_8000_0000_0000_0200), 1)
    }

    fn registry() -> SchemaRegistry {
        let mut registry = SchemaRegistry::new();
        registry
            .register(
                ComponentSchema::new(schema_ref(), "Position")
                    .with_field(FieldSchema::new(
                        field_id(1),
                        "x",
                        FieldType::F32,
                        FieldFlags::saved(),
                    ))
                    .with_field(FieldSchema::new(
                        field_id(2),
                        "y",
                        FieldType::F32,
                        FieldFlags::saved(),
                    )),
            )
            .unwrap();
        registry
    }

    fn delta(entity_seed: u128, fields: Vec<FieldValue>) -> ComponentDelta {
        let mut delta = ComponentDelta::new(EntityUuid(Uuid::from_u128(entity_seed)), schema_ref());
        delta.fields = fields;
        delta
    }

    #[test]
    fn journal_is_append_only_and_canonical() {
        let mut journal = SaveJournal::new();
        let first = journal.append(
            SimTick::ZERO,
            vec![
                delta(
                    2,
                    vec![
                        FieldValue::new(field_id(2), Value::F32(2.0)),
                        FieldValue::new(field_id(1), Value::F32(1.0)),
                    ],
                ),
                delta(1, vec![FieldValue::new(field_id(1), Value::F32(0.5))]),
            ],
        );
        assert_eq!(first, 1);
        assert_eq!(journal.seq(), 1);

        let entry = &journal.entries[0];
        assert_eq!(entry.seq, 1);
        assert_eq!(entry.deltas[0].entity.0, Uuid::from_u128(1));
        assert_eq!(entry.deltas[1].entity.0, Uuid::from_u128(2));
        assert_eq!(entry.deltas[1].fields[0].field, field_id(1));
        assert_eq!(entry.deltas[1].fields[1].field, field_id(2));

        let second = journal.append(SimTick::ZERO, Vec::new());
        assert_eq!(second, 2);
        assert_eq!(journal.len(), 2);
    }

    #[test]
    fn ron_roundtrip_is_stable() {
        let mut journal = SaveJournal::new();
        journal.append(
            SimTick::ZERO,
            vec![delta(
                3,
                vec![
                    FieldValue::new(field_id(1), Value::F32(1.5)),
                    FieldValue::new(field_id(2), Value::F32(-2.5)),
                ],
            )],
        );

        let text = ron::to_string(&journal).unwrap();
        let restored: SaveJournal = ron::from_str(&text).unwrap();
        assert_eq!(restored, journal);
        assert_eq!(ron::to_string(&restored).unwrap(), text);
    }

    #[test]
    fn unknown_fields_are_logged_not_lost() {
        let registry = registry();
        let mut journal = SaveJournal::new();
        journal.append(
            SimTick::ZERO,
            vec![delta(
                4,
                vec![
                    FieldValue::new(field_id(1), Value::F32(1.0)),
                    FieldValue::new(field_id(99), Value::F32(9.0)),
                ],
            )],
        );

        let removed = journal.sanitize(&registry);
        assert_eq!(removed, 1);
        assert_eq!(journal.entries[0].deltas[0].fields.len(), 1);
        assert_eq!(
            journal.orphans(),
            &[OrphanedField {
                entity: EntityUuid(Uuid::from_u128(4)),
                schema: schema_ref(),
                field: field_id(99),
            }]
        );
    }
}
