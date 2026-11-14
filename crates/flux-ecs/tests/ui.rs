//! Compile-fail: `Reflect` обязан отвергать неподдерживаемые формы на этапе
//! компиляции, а не генерировать случайные UUID (ADR-0030).

#[test]
fn reflect_derive_rejects_unsupported_shapes() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
