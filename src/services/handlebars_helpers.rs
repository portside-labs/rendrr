//! Handlebars helpers and data-validation routines used by the template
//! engine.
//!
//! The helpers (`table`, `image`, `chunk`) are format-agnostic — they emit
//! either an empty string (control-flow markers) or a placeholder string that
//! the engine resolves after the Handlebars pass.

use crate::errors::RenderError;
use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext, Renderable};

/// Register the shared helpers (`table`, `image`, `chunk`) on a Handlebars
/// instance. The engine calls this from its constructor.
pub fn register_shared_helpers(handlebars: &mut Handlebars<'static>) {
    handlebars.set_strict_mode(false);
    handlebars.register_helper("table", Box::new(table_helper));
    handlebars.register_helper("image", Box::new(insert_image_helper));
    handlebars.register_helper("chunk", Box::new(chunk_helper));
}

/// No-op helper. The row-iteration mechanic is implemented in the engine
/// by rewriting `{{#each}}` blocks so they span the entire container element
/// (`<w:tr>` for table rows).
fn table_helper(
    _h: &Helper,
    _: &Handlebars,
    _: &Context,
    _rc: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    out.write("")?;
    Ok(())
}

/// `{{#chunk items 2}}...{{/chunk}}` — splits an array into groups of N for
/// grid layouts.
fn chunk_helper(
    h: &Helper,
    r: &Handlebars,
    _ctx: &Context,
    rc: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let array = h
        .param(0)
        .and_then(|v| v.value().as_array())
        .ok_or_else(|| {
            handlebars::RenderError::from(handlebars::RenderErrorReason::Other(
                "chunk helper requires an array as first parameter".to_string(),
            ))
        })?;

    let chunk_size = h.param(1).and_then(|v| v.value().as_u64()).ok_or_else(|| {
        handlebars::RenderError::from(handlebars::RenderErrorReason::Other(
            "chunk helper requires a number as second parameter".to_string(),
        ))
    })? as usize;

    if chunk_size == 0 {
        return Err(handlebars::RenderError::from(
            handlebars::RenderErrorReason::Other("chunk size must be greater than 0".to_string()),
        ));
    }

    let template = h.template();
    if template.is_none() {
        return Ok(());
    }

    for chunk in array.chunks(chunk_size) {
        let chunk_value = serde_json::Value::Array(chunk.to_vec());

        let mut block_rc = rc.clone();
        let mut block = handlebars::BlockContext::new();
        block.set_base_value(chunk_value);
        block_rc.push_block(block);

        if let Some(t) = template {
            let rendered = t.renders(r, _ctx, &mut block_rc)?;
            out.write(&rendered)?;
        }
    }

    Ok(())
}

/// `{{image url}}`, `{{image url width=400}}`, etc. — emits a placeholder
/// that the engine resolves into format-specific embed XML after Handlebars
/// rendering.
///
/// Placeholder format: `___IMAGE_PLACEHOLDER|||url|||width|||height___`
/// (`|||` as delimiter to avoid colliding with `:` in URLs).
fn insert_image_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _rc: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let url = h.param(0).and_then(|v| v.value().as_str()).ok_or_else(|| {
        handlebars::RenderError::from(handlebars::RenderErrorReason::Other(
            "image helper requires a URL parameter".to_string(),
        ))
    })?;

    let width = h
        .hash_get("width")
        .and_then(|v| v.value().as_u64())
        .map(|w| w as u32);

    let height = h
        .hash_get("height")
        .and_then(|v| v.value().as_u64())
        .map(|h| h as u32);

    let width_str = width.map(|w| w.to_string()).unwrap_or_default();
    let height_str = height.map(|h| h.to_string()).unwrap_or_default();
    let placeholder = format!(
        "___IMAGE_PLACEHOLDER|||{}|||{}|||{}___",
        url, width_str, height_str
    );

    out.write(&placeholder)?;
    Ok(())
}

/// Bound the depth and array sizes in the user-provided JSON to prevent
/// stack overflow and resource exhaustion.
pub fn validate_data_structure(data: &serde_json::Value) -> Result<(), RenderError> {
    check_depth_and_size(data, 0)
}

fn check_depth_and_size(value: &serde_json::Value, depth: usize) -> Result<(), RenderError> {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return Err(RenderError::DataValidation(format!(
            "Data nesting depth exceeds maximum of {} levels",
            MAX_DEPTH
        )));
    }

    match value {
        serde_json::Value::Object(map) => {
            for (_key, val) in map {
                check_depth_and_size(val, depth + 1)?;
            }
        }
        serde_json::Value::Array(arr) => {
            const MAX_ARRAY_SIZE: usize = 10000;
            if arr.len() > MAX_ARRAY_SIZE {
                return Err(RenderError::DataValidation(format!(
                    "Array size {} exceeds maximum of {}",
                    arr.len(),
                    MAX_ARRAY_SIZE
                )));
            }
            for item in arr {
                check_depth_and_size(item, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
