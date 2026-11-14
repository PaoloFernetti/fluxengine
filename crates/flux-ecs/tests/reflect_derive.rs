//! I4: `#[derive(Reflect)]` заполняет схему и `Component::TYPE_ID`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use flux_core::schema::{FieldFlags, TypeId};
use flux_ecs::component::Component;
use flux_ecs::schema::{FieldType, OrphanPolicy, SchemaRegistry};
use flux_macros::Reflect;
use uuid::Uuid;

const TRANSFORM_UUID: &str = "0b0e0d0c-0a09-0807-0605-0403020100ff";
const HIDDEN_UUID: &str = "0b0e0d0c-0a09-0807-0605-0403020100aa";

#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(
    type_uuid = "0b0e0d0c-0a09-0807-0605-0403020100ff",
    schema_rev = 2,
    orphan = "Respawn"
)]
struct Transform {
    #[save]
    #[reflect(min = -1000.0, max = 1000.0, slider, category = "Transform", tooltip = "X")]
    x: f32,
    #[save]
    y: f32,
    #[replicated]
    velocity: [f32; 3],
    #[field(uuid = "0b0e0d0c-0a09-0807-0605-0403020100aa")]
    hidden: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
struct Plain {
    value: i32,
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).unwrap()
}

#[test]
fn derive_sets_type_id_and_schema() {
    let expected = TypeId::from_uuid(uuid(TRANSFORM_UUID));
    assert_eq!(Transform::TYPE_ID, expected);

    let schema = Transform::schema();
    assert_eq!(schema.schema.type_uuid, expected);
    assert_eq!(schema.schema.schema_rev, 2);
    assert_eq!(schema.name, "Transform");
    assert_eq!(schema.orphan_policy, OrphanPolicy::Respawn);
    assert_eq!(schema.fields.len(), 4);

    let names: Vec<&str> = schema.fields.iter().map(|field| field.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names.len(), sorted.len());
    // Канонический порядок — по field_uuid.
    let uuids: Vec<Uuid> = schema
        .fields
        .iter()
        .map(|field| field.field_uuid.0)
        .collect();
    let mut sorted_uuids = uuids.clone();
    sorted_uuids.sort_unstable();
    assert_eq!(uuids, sorted_uuids);
}

#[test]
fn field_flags_types_and_editor_metadata() {
    let schema = Transform::schema();
    let x = schema
        .fields
        .iter()
        .find(|field| field.name == "x")
        .unwrap();
    assert_eq!(x.ty, FieldType::F32);
    assert_eq!(x.flags, FieldFlags::saved());
    assert_eq!(x.editor.min, Some(-1000.0));
    assert_eq!(x.editor.max, Some(1000.0));
    assert!(x.editor.slider);
    assert_eq!(x.editor.category.as_deref(), Some("Transform"));
    assert_eq!(x.editor.tooltip.as_deref(), Some("X"));
    assert!(x.editor.visible);

    let velocity = schema
        .fields
        .iter()
        .find(|field| field.name == "velocity")
        .unwrap();
    assert_eq!(velocity.ty, FieldType::Vec3);
    assert_eq!(velocity.flags, FieldFlags::replicated());

    let hidden = schema
        .fields
        .iter()
        .find(|field| field.name == "hidden")
        .unwrap();
    assert_eq!(hidden.field_uuid.0, uuid(HIDDEN_UUID));
    assert_eq!(hidden.ty, FieldType::Bool);
    assert_eq!(hidden.flags, FieldFlags::transient());

    assert_eq!(schema.save_fields().count(), 3);
    assert_eq!(schema.replicated_fields().count(), 1);
}

#[test]
fn field_uuid_is_deterministic_seed_from_type_and_name() {
    let schema = Transform::schema();
    let y = schema
        .fields
        .iter()
        .find(|field| field.name == "y")
        .unwrap();
    let expected = Uuid::new_v5(&uuid(TRANSFORM_UUID), b"y");
    assert_eq!(y.field_uuid.0, expected);
}

#[test]
fn plain_struct_gets_transient_field_and_v5_type_uuid() {
    let schema = Plain::schema();
    assert_eq!(schema.schema.schema_rev, 1);
    assert_eq!(schema.fields.len(), 1);
    assert_eq!(schema.fields[0].ty, FieldType::I32);
    assert_eq!(schema.fields[0].flags, FieldFlags::transient());
    assert_eq!(schema.orphan_policy, OrphanPolicy::Log);
    assert_eq!(
        Plain::TYPE_ID.0,
        Uuid::new_v5(&Uuid::NAMESPACE_OID, b"Plain")
    );
}

#[test]
fn schema_registers_in_registry() {
    let mut registry = SchemaRegistry::new();
    registry.register(Transform::schema()).unwrap();
    registry.register(Plain::schema()).unwrap();

    assert_eq!(registry.len(), 2);
    let stored = registry.get(Transform::TYPE_ID).unwrap();
    assert_eq!(stored.name, "Transform");
    assert!(registry.get(Plain::TYPE_ID).is_some());
}
