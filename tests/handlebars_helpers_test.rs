//! Direct tests for the `table` / `image` / `chunk` helpers and the
//! data-structure guard. These are exercised indirectly by the render tests,
//! but going through Handlebars directly is the only way to reach their
//! failure branches — a render test can only observe the collapsed result.

use handlebars::Handlebars;
use rendrr::services::handlebars_helpers::{register_shared_helpers, validate_data_structure};
use serde_json::json;

fn engine() -> Handlebars<'static> {
    let mut hb = Handlebars::new();
    register_shared_helpers(&mut hb);
    hb
}

// ---------------- chunk ----------------

#[test]
fn chunk_groups_an_array_into_fixed_size_batches() {
    let out = engine()
        .render_template(
            "{{#chunk items 2}}[{{#each this}}{{this}}{{/each}}]{{/chunk}}",
            &json!({"items": ["a", "b", "c", "d"]}),
        )
        .unwrap();
    assert_eq!(out, "[ab][cd]");
}

#[test]
fn chunk_emits_a_partial_final_group() {
    let out = engine()
        .render_template(
            "{{#chunk items 2}}[{{#each this}}{{this}}{{/each}}]{{/chunk}}",
            &json!({"items": ["a", "b", "c"]}),
        )
        .unwrap();
    assert_eq!(out, "[ab][c]");
}

#[test]
fn chunk_on_an_empty_array_emits_nothing() {
    let out = engine()
        .render_template("{{#chunk items 2}}x{{/chunk}}", &json!({"items": []}))
        .unwrap();
    assert_eq!(out, "");
}

#[test]
fn chunk_rejects_a_non_array_first_parameter() {
    let err = engine()
        .render_template("{{#chunk items 2}}x{{/chunk}}", &json!({"items": "nope"}))
        .unwrap_err()
        .to_string();
    assert!(err.contains("array"), "unexpected: {err}");
}

#[test]
fn chunk_rejects_a_missing_size_parameter() {
    let err = engine()
        .render_template("{{#chunk items}}x{{/chunk}}", &json!({"items": [1, 2]}))
        .unwrap_err()
        .to_string();
    assert!(err.contains("number"), "unexpected: {err}");
}

#[test]
fn chunk_rejects_a_zero_size() {
    // Without this guard `array.chunks(0)` panics.
    let err = engine()
        .render_template("{{#chunk items 0}}x{{/chunk}}", &json!({"items": [1, 2]}))
        .unwrap_err()
        .to_string();
    assert!(err.contains("greater than 0"), "unexpected: {err}");
}

// ---------------- image ----------------

#[test]
fn image_emits_a_placeholder_carrying_the_url() {
    let out = engine()
        .render_template("{{image logo}}", &json!({"logo": "https://x.test/a.png"}))
        .unwrap();
    assert!(out.starts_with("___IMAGE_PLACEHOLDER|||"), "got {out}");
    assert!(out.contains("https://x.test/a.png"));
}

#[test]
fn image_placeholder_carries_width_and_height_when_given() {
    let out = engine()
        .render_template(
            "{{image logo width=300 height=200}}",
            &json!({"logo": "https://x.test/a.png"}),
        )
        .unwrap();
    assert!(out.contains("|||300|||200___"), "got {out}");
}

#[test]
fn image_placeholder_leaves_dimensions_blank_when_omitted() {
    let out = engine()
        .render_template("{{image logo}}", &json!({"logo": "https://x.test/a.png"}))
        .unwrap();
    assert_eq!(out, "___IMAGE_PLACEHOLDER|||https://x.test/a.png||||||___");
}

#[test]
fn image_placeholder_has_the_exact_shape_the_engine_parses_back() {
    // The engine recovers url/width/height from this string with a regex, so
    // the delimiter layout is a contract between the two, not cosmetic.
    let out = engine()
        .render_template(
            "{{image logo width=300}}",
            &json!({"logo": "https://x.test/a.png"}),
        )
        .unwrap();
    assert_eq!(
        out,
        "___IMAGE_PLACEHOLDER|||https://x.test/a.png|||300|||___"
    );
}

#[test]
fn image_rejects_a_missing_url() {
    let err = engine()
        .render_template("{{image}}", &json!({}))
        .unwrap_err()
        .to_string();
    assert!(err.contains("URL"), "unexpected: {err}");
}

// ---------------- table ----------------

#[test]
fn table_helper_is_inert() {
    // Row repetition happens in the engine's XML pass, not here; the helper
    // exists so `{{#table}}` parses without erroring.
    let out = engine()
        .render_template("a{{table rows}}b", &json!({"rows": [1, 2]}))
        .unwrap();
    assert_eq!(out, "ab");
}

// ---------------- data structure limits ----------------

#[test]
fn accepts_ordinary_payloads() {
    assert!(validate_data_structure(&json!({
        "name": "Acme",
        "items": [{"sku": "A", "qty": 2}],
        "nested": {"a": {"b": {"c": 1}}}
    }))
    .is_ok());
}

#[test]
fn accepts_scalars_and_null() {
    for v in [json!(null), json!(1), json!("s"), json!(true), json!([])] {
        assert!(validate_data_structure(&v).is_ok(), "rejected {v}");
    }
}

#[test]
fn rejects_data_nested_past_the_depth_limit() {
    // Deep nesting is the stack-overflow vector — the guard exists so a
    // hostile payload can't recurse the renderer off the stack.
    let mut v = json!("leaf");
    for _ in 0..200 {
        v = json!({ "next": v });
    }
    let err = validate_data_structure(&v).unwrap_err().to_string();
    assert!(err.contains("depth"), "unexpected: {err}");
}

#[test]
fn rejects_arrays_past_the_size_limit() {
    let big = json!({ "items": vec![0u32; 10_001] });
    let err = validate_data_structure(&big).unwrap_err().to_string();
    assert!(err.contains("Array size"), "unexpected: {err}");
}

#[test]
fn accepts_an_array_exactly_at_the_limit() {
    let at_limit = json!({ "items": vec![0u32; 10_000] });
    assert!(validate_data_structure(&at_limit).is_ok());
}

#[test]
fn finds_an_oversized_array_nested_inside_an_object() {
    // The walk has to recurse, not just check the top level.
    let nested = json!({"a": {"b": {"c": vec![0u32; 10_001]}}});
    assert!(validate_data_structure(&nested).is_err());
}

// ---------------- CORS layer construction ----------------

#[test]
fn cors_layers_build_without_panicking() {
    // Both constructors are called at startup; a bad layer config panics there
    // rather than returning an error, so exercise them here instead.
    let _ = rendrr::permissive_cors();
    let _ = rendrr::cors_layer_from_env();
}
