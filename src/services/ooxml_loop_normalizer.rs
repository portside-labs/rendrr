//! Loop normalizer for OOXML container elements.
//!
//! Word splits user-typed Handlebars control blocks (`{{#each}}`,
//! `{{/each}}`, `{{#if}}`, `{{/if}}`, `{{#chunk}}`, `{{/chunk}}`,
//! `{{else}}`) across container element boundaries — most commonly table
//! rows (`<w:tr>`). When a user wants `{{#each rows}}` to repeat a whole
//! table row, they place the opening token in the first cell of one row and
//! the closing token in the last cell of the same row (or sometimes the
//! next/previous row).
//!
//! This module rewrites those input rows so the control tag sits *outside*
//! the container element, letting Handlebars repeat the entire row when
//! it evaluates `{{#each}}`.
//!
//! The algorithm is parameterized on the container element name rather than
//! hard-coded to `w:tr`, so it applies unchanged to any OOXML repeating
//! container.

use crate::errors::RenderError;
use regex::Regex;
use std::sync::LazyLock;

static EXPR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{[^\}]+\}\}").unwrap());
static EACH_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{#each\s+[^\}]+\}\}").unwrap());
static EACH_CLOSE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{/each\}\}").unwrap());
static CHUNK_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{#chunk\s+[^\}]+\}\}").unwrap());
static CHUNK_CLOSE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{/chunk\}\}").unwrap());
static IF_OPEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{#if\s+[^\}]+\}\}").unwrap());
static IF_CLOSE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{/if\}\}").unwrap());
static ELSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{else(\s+if\s+[^\}]+)?\}\}").unwrap());

/// Rewrite the XML so that any control tag (`{{#each}}`, `{{/each}}`, etc.)
/// that lives alone inside a container element of name `container` is lifted
/// outside that container — opening tags go before the next container,
/// closing/continuation tags go after the previous container.
pub fn normalize_container_loops(xml: &str, container: &str) -> Result<String, RenderError> {
    // Build a regex matching one full container element (lazy match, single line).
    let container_re = Regex::new(&format!(r"<{c}[^>]*>.*?</{c}>", c = container))
        .map_err(|e| RenderError::TemplateProcessing(format!("regex build failed: {}", e)))?;
    let open_marker = format!("<{}", container);
    let close_marker = format!("</{}>", container);

    let mut result = xml.to_string();

    // Collect rows that contain exactly one control marker.
    let mut rows_to_process: Vec<(usize, usize, String, bool)> = Vec::new();

    for row_match in container_re.find_iter(&result) {
        let row_text = row_match.as_str();
        let total_exprs = EXPR.find_iter(row_text).count();
        if total_exprs != 1 {
            continue;
        }

        let entry = if let Some(m) = EACH_OPEN.find(row_text) {
            Some((m.as_str().to_string(), true))
        } else if let Some(m) = EACH_CLOSE.find(row_text) {
            Some((m.as_str().to_string(), false))
        } else if let Some(m) = CHUNK_OPEN.find(row_text) {
            Some((m.as_str().to_string(), true))
        } else if let Some(m) = CHUNK_CLOSE.find(row_text) {
            Some((m.as_str().to_string(), false))
        } else if let Some(m) = IF_OPEN.find(row_text) {
            Some((m.as_str().to_string(), true))
        } else if let Some(m) = ELSE.find(row_text) {
            Some((m.as_str().to_string(), false))
        } else {
            IF_CLOSE
                .find(row_text)
                .map(|m| (m.as_str().to_string(), false))
        };

        if let Some((tag, is_opening)) = entry {
            rows_to_process.push((row_match.start(), row_match.end(), tag, is_opening));
        }
    }

    // Process in reverse so earlier offsets remain valid.
    rows_to_process.reverse();

    for (row_start, row_end, tag, is_opening) in rows_to_process {
        if is_opening {
            // Insert before the NEXT container start, then remove this row.
            if let Some(next_pos_rel) = result[row_end..].find(&open_marker) {
                let insert_pos = row_end + next_pos_rel;
                result.replace_range(row_start..row_end, "");
                let adjusted_insert = insert_pos - (row_end - row_start);
                result.insert_str(adjusted_insert, &tag);
            }
        } else {
            // Insert after the PREVIOUS container end, then remove this row.
            if let Some(prev_pos) = result[..row_start].rfind(&close_marker) {
                let insert_pos = prev_pos + close_marker.len();
                result.insert_str(insert_pos, &tag);
                let adjusted_start = row_start + tag.len();
                let adjusted_end = row_end + tag.len();
                result.replace_range(adjusted_start..adjusted_end, "");
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifts_each_open_before_next_row() {
        let xml = r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>{{#each items}}</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>{{this.name}}</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
        let out = normalize_container_loops(xml, "w:tr").unwrap();
        // Each-open should now appear before the surviving <w:tr> (the data row)
        assert!(out.contains("{{#each items}}<w:tr"));
        // Only one <w:tr> remains (the data row); the marker row was removed
        assert_eq!(out.matches("<w:tr").count(), 1);
    }

    #[test]
    fn lifts_each_close_after_prev_row() {
        let xml = r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>{{this.name}}</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>{{/each}}</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
        let out = normalize_container_loops(xml, "w:tr").unwrap();
        assert!(out.contains("</w:tr>{{/each}}"));
        assert_eq!(out.matches("<w:tr").count(), 1);
    }

    #[test]
    fn container_name_is_not_hard_coded_to_w_tr() {
        // The algorithm is parameterized on the container element name.
        let xml = r#"<x:tbl><x:row><x:c><x:t>{{#each rows}}</x:t></x:c></x:row><x:row><x:c><x:t>{{this}}</x:t></x:c></x:row></x:tbl>"#;
        let out = normalize_container_loops(xml, "x:row").unwrap();
        assert!(out.contains("{{#each rows}}<x:row"));
        assert_eq!(out.matches("<x:row").count(), 1);
    }

    #[test]
    fn lifts_if_open_and_close_around_intervening_containers() {
        let xml = r#"<w:tbl><w:tr><w:tc><w:t>{{#if visible}}</w:t></w:tc></w:tr><w:tr><w:tc><w:t>card</w:t></w:tc></w:tr><w:tr><w:tc><w:t>{{/if}}</w:t></w:tc></w:tr></w:tbl>"#;
        let out = normalize_container_loops(xml, "w:tr").unwrap();
        assert!(out.contains("{{#if visible}}<w:tr"));
        assert!(out.contains("</w:tr>{{/if}}"));
    }

    #[test]
    fn ignores_rows_with_multiple_expressions() {
        // Row contains both a control AND a data expression — should be left alone
        let xml = r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>{{#each items}}{{x}}</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
        let out = normalize_container_loops(xml, "w:tr").unwrap();
        assert_eq!(out, xml);
    }
}
