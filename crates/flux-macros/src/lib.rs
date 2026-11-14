//! Служебные дерайвы FluxEngine (ADR-0028).
//!
//! `#[derive(Reflect)]` генерирует `impl Component` и `fn schema()` с полным
//! [`ComponentSchema`]: тип/поле UUID, `FieldType`, `FieldFlags`, `EditorMeta`.
//! UUID не генерируются случайно: по умолчанию это детерминированный UUID v5
//! от имени типа/поля; явное значение задаётся атрибутом.
//!
//! ```ignore
//! #[derive(Default, Reflect)]
//! #[reflect(type_uuid = "0b0e0d0c-0a09-0807-0605-0403020100ff")]
//! struct Position {
//!     #[save]
//!     #[reflect(min = -1000.0, max = 1000.0, slider)]
//!     x: f32,
//!     #[save]
//!     y: f32,
//!     #[replicated]
//!     velocity: [f32; 3],
//! }
//! ```
//!
//! Генерируемый код — ABI схемы: ренейм поля меняет только `name`, UUID
//! сохраняется; смена типа — новая `schema_rev` (ADR-0030).

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Lit, LitStr};
use uuid::Uuid;

/// Дерайв схемы компонента: `Component` + `schema()`.
#[proc_macro_derive(Reflect, attributes(reflect, field, save, replicated))]
pub fn derive_reflect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[derive(Default)]
struct EditorAttrs {
    min: Option<f64>,
    max: Option<f64>,
    slider: bool,
    category: Option<String>,
    tooltip: Option<String>,
    visible: bool,
}

fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let ident = input.ident;
    let name = ident.to_string();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut type_uuid: Option<Uuid> = None;
    let mut schema_rev: u32 = 1;
    let mut orphan = String::from("Log");

    for attr in &input.attrs {
        if !attr.path().is_ident("reflect") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("type_uuid") {
                let literal: LitStr = meta.value()?.parse()?;
                type_uuid = Some(parse_uuid(&literal)?);
                Ok(())
            } else if meta.path.is_ident("schema_rev") {
                let literal: syn::LitInt = meta.value()?.parse()?;
                schema_rev = literal.base10_parse()?;
                Ok(())
            } else if meta.path.is_ident("orphan") {
                let literal: LitStr = meta.value()?.parse()?;
                orphan = literal.value();
                Ok(())
            } else {
                Err(meta.error("unknown container attribute (type_uuid, schema_rev, orphan)"))
            }
        })?;
    }

    let type_uuid =
        type_uuid.unwrap_or_else(|| Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes()));

    let orphan_policy = match orphan.as_str() {
        "Respawn" => quote!(::flux_ecs::schema::OrphanPolicy::Respawn),
        "Drop" => quote!(::flux_ecs::schema::OrphanPolicy::Drop),
        _ => quote!(::flux_ecs::schema::OrphanPolicy::Log),
    };

    let data = match input.data {
        Data::Struct(data) => data,
        _ => {
            return Err(syn::Error::new_spanned(
                &ident,
                "Reflect поддерживает только struct с именованными полями",
            ))
        }
    };
    let Fields::Named(fields) = data.fields else {
        return Err(syn::Error::new_spanned(
            &ident,
            "Reflect поддерживает только struct с именованными полями",
        ));
    };

    let mut field_tokens = Vec::new();
    for field in fields.named {
        let Some(field_ident) = field.ident else {
            continue;
        };
        let field_name = field_ident.to_string();

        let mut field_uuid: Option<Uuid> = None;
        let mut save = false;
        let mut replicated = false;
        let mut editor = EditorAttrs {
            visible: true,
            ..EditorAttrs::default()
        };

        for attr in &field.attrs {
            if attr.path().is_ident("field") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("uuid") {
                        let literal: LitStr = meta.value()?.parse()?;
                        field_uuid = Some(parse_uuid(&literal)?);
                        Ok(())
                    } else {
                        Err(meta.error("unknown field attribute (uuid)"))
                    }
                })?;
            } else if attr.path().is_ident("save") {
                save = true;
            } else if attr.path().is_ident("replicated") {
                replicated = true;
                save = true;
            } else if attr.path().is_ident("reflect") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("min") {
                        editor.min = Some(parse_f64(meta.value()?.parse()?)?);
                        Ok(())
                    } else if meta.path.is_ident("max") {
                        editor.max = Some(parse_f64(meta.value()?.parse()?)?);
                        Ok(())
                    } else if meta.path.is_ident("slider") {
                        editor.slider = true;
                        Ok(())
                    } else if meta.path.is_ident("category") {
                        let literal: LitStr = meta.value()?.parse()?;
                        editor.category = Some(literal.value());
                        Ok(())
                    } else if meta.path.is_ident("tooltip") {
                        let literal: LitStr = meta.value()?.parse()?;
                        editor.tooltip = Some(literal.value());
                        Ok(())
                    } else if meta.path.is_ident("visible") {
                        if meta.input.peek(syn::Token![=]) {
                            let literal: syn::LitBool = meta.value()?.parse()?;
                            editor.visible = literal.value();
                        } else {
                            editor.visible = true;
                        }
                        Ok(())
                    } else {
                        Err(meta.error(
                            "unknown field reflect attribute (min, max, slider, category, tooltip, visible)",
                        ))
                    }
                })?;
            }
        }

        let field_uuid =
            field_uuid.unwrap_or_else(|| Uuid::new_v5(&type_uuid, field_name.as_bytes()));
        let field_type = field_type(&field.ty);
        let flags = if replicated {
            quote!(::flux_core::schema::FieldFlags::replicated())
        } else if save {
            quote!(::flux_core::schema::FieldFlags::saved())
        } else {
            quote!(::flux_core::schema::FieldFlags::transient())
        };

        let min = option_f64(editor.min);
        let max = option_f64(editor.max);
        let category = option_string(editor.category);
        let tooltip = option_string(editor.tooltip);
        let slider = editor.slider;
        let visible = editor.visible;
        let field_uuid = field_uuid.as_u128();

        field_tokens.push(quote! {
            ::flux_ecs::schema::FieldSchema {
                field_uuid: ::flux_core::schema::FieldId(
                    ::flux_core::schema::uuid_from_u128(#field_uuid),
                ),
                name: #field_name,
                ty: #field_type,
                flags: #flags,
                editor: ::flux_ecs::schema::EditorMeta {
                    min: #min,
                    max: #max,
                    slider: #slider,
                    category: #category,
                    tooltip: #tooltip,
                    visible: #visible,
                },
                default_ron: None,
            }
        });
    }

    let type_uuid = type_uuid.as_u128();
    Ok(quote! {
        impl #impl_generics ::flux_ecs::component::Component for #ident #ty_generics #where_clause {
            const TYPE_ID: ::flux_core::schema::TypeId =
                ::flux_core::schema::TypeId::from_uuid(
                    ::flux_core::schema::uuid_from_u128(#type_uuid),
                );

            fn schema() -> ::flux_ecs::schema::ComponentSchema {
                let mut schema = ::flux_ecs::schema::ComponentSchema {
                    schema: ::flux_core::schema::SchemaRef::new(Self::TYPE_ID, #schema_rev),
                    name: #name,
                    fields: vec![#(#field_tokens),*],
                    orphan_policy: #orphan_policy,
                };
                schema.sort_fields();
                schema
            }
        }
    })
}

fn parse_uuid(literal: &LitStr) -> syn::Result<Uuid> {
    Uuid::parse_str(&literal.value())
        .map_err(|_| syn::Error::new(literal.span(), "invalid uuid literal"))
}

fn parse_f64(expression: syn::Expr) -> syn::Result<f64> {
    match expression {
        syn::Expr::Lit(literal) => match literal.lit {
            Lit::Float(value) => value.base10_parse(),
            Lit::Int(value) => value.base10_parse(),
            _ => Err(syn::Error::new_spanned(
                literal,
                "expected a number (min/max)",
            )),
        },
        syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => {
            Ok(-parse_f64(*unary.expr)?)
        }
        other => Err(syn::Error::new_spanned(
            other,
            "expected a number (min/max)",
        )),
    }
}

fn option_f64(value: Option<f64>) -> TokenStream2 {
    match value {
        Some(value) => quote!(Some(#value)),
        None => quote!(None),
    }
}

fn option_string(value: Option<String>) -> TokenStream2 {
    match value {
        Some(value) => quote!(Some(::std::string::String::from(#value))),
        None => quote!(None),
    }
}

fn field_type(ty: &syn::Type) -> TokenStream2 {
    let text = quote!(#ty).to_string().replace(' ', "");
    let variant = match text.as_str() {
        "f32" => "F32",
        "f64" => "F64",
        "i32" => "I32",
        "u32" => "U32",
        "i64" => "I64",
        "u64" => "U64",
        "bool" => "Bool",
        "String" => "String",
        "[f32;3]" => "Vec3",
        "[f64;3]" => "DVec3",
        "WorldPos" | "flux_math::WorldPos" => "WorldPos",
        "Uuid" | "uuid::Uuid" => "Uuid",
        "Vec<u8>" | "Vec::<u8>" => "Bytes",
        _ => "Other",
    };
    let variant = syn::Ident::new(variant, proc_macro2::Span::call_site());
    quote!(::flux_ecs::schema::FieldType::#variant)
}
