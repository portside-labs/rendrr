//! The DOCX template engine.
//!
//! A `.docx` is a ZIP of XML parts. Rendering unpacks the parts that can carry
//! placeholders (`word/document.xml` plus any headers and footers), normalizes
//! the Word XML so placeholders split across runs become whole again, runs the
//! text through Handlebars, resolves image placeholders into embedded media,
//! and repackages the archive.
//!
//! Nested paths, `{{#each}}`, and `{{#if}}` come from Handlebars itself; the
//! `table`, `image`, and `chunk` helpers are registered in
//! [`crate::services::handlebars_helpers`].

use crate::docx::image_embedder::ImageEmbedder;
use crate::errors::{RenderError, ValidationIssue, ValidationReport};
use crate::services::handlebars_helpers::{register_shared_helpers, validate_data_structure};
use crate::services::image_handler::{ImageHandler, ImageSource};
use crate::services::ooxml_content_types::ensure_image_content_types;
use crate::services::ooxml_loop_normalizer::normalize_container_loops;
use bytes::Bytes;
use handlebars::Handlebars;
use regex::Regex;
use std::io::{Cursor, Read, Write};
use std::sync::LazyLock;
use zip::{write::FileOptions, ZipArchive, ZipWriter};

/// Pre-compiled regex patterns used throughout the template engine.
/// Using LazyLock ensures each pattern is compiled exactly once.
mod patterns {
    use super::*;

    // -- Expression detection --
    pub static EXPR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{[^\}]+\}\}").unwrap());
    pub static EXPR_RELAXED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\{\{[^\}]*\}\}").unwrap());

    // -- XML normalization (normalize_word_xml) --
    pub static PROOF_ERR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<w:proofErr[^>]*/>").unwrap());
    pub static BOOKMARK_START: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<w:bookmarkStart[^>]*/>").unwrap());
    pub static BOOKMARK_END: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<w:bookmarkEnd[^>]*/>").unwrap());
    pub static MERGE_TEXT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"</w:t><w:t[^>]*>").unwrap());
    pub static MERGE_RUNS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"</w:t></w:r><w:r[^>]*>(<w:rPr/>|<w:rPr>.*?</w:rPr>)?<w:t[^>]*>").unwrap()
    });
    pub static MERGE_RUNS_SPACING: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"</w:r><w:r[^>]*><w:t[^>]*>").unwrap());

    // -- Paragraph block tag stripping --
    pub static PARAGRAPH: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<w:p(?:\s[^>]*[^/])?>.*?</w:p>").unwrap());
    pub static XML_TEXT_CONTENT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<w:t[^>]*>(.*?)</w:t>").unwrap());
    pub static BLOCK_TAG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^\{\{(#if\s|#each\s|#chunk\s|else\s*if\s|else|/if|/each|/chunk)").unwrap()
    });

    // -- Expression validation (validate_expressions) --
    pub static CHUNK_OPEN_FULL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\{\{#chunk\s+([^\s]+)(\s+\d+)?\}\}$").unwrap());
    pub static EACH_OPEN_FULL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\{\{#each\s+([^\s\}]+)\}\}$").unwrap());
    pub static IF_OPEN_FULL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\{\{#if\s+([^\s\}]+)\}\}$").unwrap());
    pub static CHUNK_CLOSE_FULL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\{\{/chunk\}\}$").unwrap());
    pub static EACH_CLOSE_FULL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\{\{/each\}\}$").unwrap());
    pub static IF_CLOSE_FULL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\{\{/if\}\}$").unwrap());
    pub static IMAGE_HELPER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^\{\{image\s+\S+(\s+width=(\S+))?(\s+height=(\S+))?\}\}$").unwrap()
    });

    // -- Error extraction --
    pub static ERROR_LINE_COL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"Unnamed":(\d+):(\d+)"#).unwrap());
    pub static ERROR_SINGLE_QUOTE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"'([^']+)'").unwrap());
    pub static ERROR_DOUBLE_QUOTE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());
    pub static ERROR_FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"field (\w+)").unwrap());
    pub static ERROR_PATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"path (\w+)").unwrap());

    // -- Image processing --
    pub static IMAGE_PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"___IMAGE_PLACEHOLDER\|\|\|([^\|]+)\|\|\|([^\|]*)\|\|\|([^_]*)___").unwrap()
    });
    pub static TABLE_CELL_WIDTH: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"<w:tcW[^>]*w:w="(\d+)"[^>]*>"#).unwrap());
    pub static RELATIONSHIP_ID: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"<Relationship[^>]+Id="([^"]+)""#).unwrap());
}

/// Largest a single decompressed archive entry may be.
///
/// Upload validation bounds the *compressed* template at 25MB, which says
/// nothing about its expanded size: a ZIP entry of highly repetitive XML
/// compresses by several orders of magnitude, so a well-formed template within
/// the upload limit can still expand to gigabytes and exhaust memory. Reading
/// every entry through a capped reader bounds the expansion instead of trusting
/// the header-declared size, which the archive author controls.
const MAX_ENTRY_BYTES: u64 = 100 * 1024 * 1024;

/// Read one ZIP entry, refusing to allocate past [`MAX_ENTRY_BYTES`].
fn read_entry_capped<R: Read>(reader: &mut R, name: &str) -> Result<Vec<u8>, RenderError> {
    let mut buf = Vec::new();
    let read = reader
        .take(MAX_ENTRY_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| RenderError::TemplateProcessing(format!("Failed to read {}: {}", name, e)))?;

    if read as u64 > MAX_ENTRY_BYTES {
        return Err(RenderError::TemplateProcessing(format!(
            "Archive entry {} expands beyond the {}MB limit; refusing to decompress it",
            name,
            MAX_ENTRY_BYTES / (1024 * 1024)
        )));
    }

    Ok(buf)
}

/// [`read_entry_capped`], decoded as UTF-8.
fn read_entry_capped_string<R: Read>(reader: &mut R, name: &str) -> Result<String, RenderError> {
    let bytes = read_entry_capped(reader, name)?;
    String::from_utf8(bytes)
        .map_err(|e| RenderError::TemplateProcessing(format!("{} is not valid UTF-8: {}", name, e)))
}

pub struct DocxTemplateEngine {
    handlebars: Handlebars<'static>,
}

impl DocxTemplateEngine {
    /// Create a new DocxTemplateEngine instance
    pub fn new() -> Self {
        let mut handlebars = Handlebars::new();
        register_shared_helpers(&mut handlebars);
        Self { handlebars }
    }

    /// Render a DOCX template with data
    pub async fn render(
        &self,
        template_data: Bytes,
        data: serde_json::Value,
    ) -> Result<Bytes, RenderError> {
        // Bound depth and array sizes before Handlebars walks the data.
        validate_data_structure(&data)?;

        // Step 1: Extract DOCX as ZIP archive
        let cursor = Cursor::new(template_data.to_vec());
        let mut archive = ZipArchive::new(cursor).map_err(|e| {
            RenderError::TemplateProcessing(format!("Failed to read DOCX archive: {}", e))
        })?;

        // Step 2: Identify all files that need rendering (main doc + headers + footers)
        let files_to_render = self.identify_renderable_files(&mut archive)?;

        // Step 3: Process each file (document.xml, headers, footers)
        let mut rendered_files = std::collections::HashMap::new();

        for file_name in &files_to_render {
            // Extract XML
            let xml = self.extract_xml_file(&mut archive, file_name)?;

            // Normalize (merge split runs)
            let normalized_xml = self.normalize_word_xml(&xml)?;

            // Render with Handlebars. Nested paths, loops, and conditionals are
            // native; strict_mode(false) leaves missing paths blank rather than
            // erroring. Headers and footers get the same data context as the body.
            let rendered_xml = self
                .handlebars
                .render_template(&normalized_xml, &data)
                .map_err(|e| {
                    // Extract position from error and show context
                    let error_msg = e.to_string();
                    if let Some((line, col)) = self.extract_line_column(&error_msg) {
                        let context = self.extract_error_context(&normalized_xml, line, col);
                        tracing::error!("Error context around line {}, column {}:", line, col);
                        tracing::error!("{}", context);
                    }

                    // Tier 2 validation: When rendering fails, provide structured error report
                    // Re-validate syntax to catch any syntax errors
                    let syntax_errors = {
                        let expressions: Vec<&str> = patterns::EXPR
                            .find_iter(&normalized_xml)
                            .map(|m| m.as_str())
                            .collect();
                        self.validate_expressions(&expressions, file_name)
                    };

                    // Parse render error to get data validation issues
                    let data_errors = self.parse_render_error(&e, file_name);

                    // Create comprehensive validation report
                    let report = ValidationReport::new_render(syntax_errors, data_errors);
                    let report_json =
                        serde_json::to_string(&report).unwrap_or_else(|_| "{}".to_string());

                    // Return with special prefix so ApiError can detect and parse it
                    RenderError::RenderingFailed(format!("VALIDATION:{}", report_json))
                })?;

            rendered_files.insert(file_name.clone(), rendered_xml);
        }

        // Step 3.5: Resolve image placeholders into embedded media
        let (rendered_files, image_files) = self.process_image_placeholders(rendered_files).await?;

        // Step 4: Repackage as DOCX with all rendered files and images
        let rendered_docx =
            self.repackage_docx_with_files_and_images(archive, rendered_files, image_files)?;

        Ok(Bytes::from(rendered_docx))
    }

    /// Identify all XML parts that need rendering (document, headers, footers).
    fn identify_renderable_files(
        &self,
        archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    ) -> Result<Vec<String>, RenderError> {
        let mut files = vec!["word/document.xml".to_string()];

        // Find all header and footer files
        for i in 0..archive.len() {
            let file = archive.by_index(i).map_err(|e| {
                RenderError::TemplateProcessing(format!("Failed to read archive: {}", e))
            })?;
            let name = file.name().to_string();

            // Add headers and footers
            if (name.starts_with("word/header") || name.starts_with("word/footer"))
                && name.ends_with(".xml")
            {
                files.push(name);
            }
        }

        Ok(files)
    }

    /// Extract any XML file from the archive
    fn extract_xml_file(
        &self,
        archive: &mut ZipArchive<Cursor<Vec<u8>>>,
        file_name: &str,
    ) -> Result<String, RenderError> {
        let mut file = archive.by_name(file_name).map_err(|e| {
            RenderError::TemplateProcessing(format!("Missing {}: {}", file_name, e))
        })?;

        read_entry_capped(&mut file, file_name).and_then(|bytes| {
            String::from_utf8(bytes).map_err(|e| {
                RenderError::TemplateProcessing(format!("{} is not valid UTF-8: {}", file_name, e))
            })
        })
    }

    /// Normalize Word XML by merging split text runs
    /// Word often splits {{variable}} across multiple <w:t> tags
    /// We need to merge them so Handlebars can find the patterns
    fn normalize_word_xml(&self, xml: &str) -> Result<String, RenderError> {
        let mut result = xml.to_string();

        // Pattern 0: Remove grammar/spell check markers that split text runs
        // These include <w:proofErr> elements that Word inserts
        result = patterns::PROOF_ERR.replace_all(&result, "").to_string();

        // Also remove bookmark start/end markers that can split expressions
        result = patterns::BOOKMARK_START
            .replace_all(&result, "")
            .to_string();
        result = patterns::BOOKMARK_END.replace_all(&result, "").to_string();

        // Run merge patterns multiple times until XML stabilizes
        // This is needed because after one merge, new merge opportunities may appear
        let mut prev_result = String::new();
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 10;

        while result != prev_result && iterations < MAX_ITERATIONS {
            prev_result = result.clone();
            iterations += 1;

            // Pattern 1: Merge text elements within the same run
            result = patterns::MERGE_TEXT.replace_all(&result, "").to_string();

            // Pattern 2: Merge runs that only contain text (no formatting changes)
            result = patterns::MERGE_RUNS.replace_all(&result, "").to_string();

            // Pattern 3: Handle runs with spacing/formatting in between
            result = patterns::MERGE_RUNS_SPACING
                .replace_all(&result, "")
                .to_string();
        }

        if iterations > 1 {
            tracing::debug!(
                "XML normalization took {} iterations to stabilize",
                iterations
            );
        }

        // Debug: Log Handlebars expressions found after normalization
        let expressions: Vec<&str> = patterns::EXPR_RELAXED
            .find_iter(&result)
            .map(|m| m.as_str())
            .collect();

        if !expressions.is_empty() {
            tracing::debug!(
                "Found {} Handlebars expressions after normalization",
                expressions.len()
            );
            for (i, expr) in expressions.iter().enumerate() {
                if i < 10 {
                    // Only log first 10 to avoid spam
                    tracing::debug!("  Expression {}: {}", i + 1, expr);
                }
            }
        }

        // Look for potential incomplete expressions (signs of split expressions we didn't catch)
        let incomplete_open = result.matches("{{").count();
        let incomplete_close = result.matches("}}").count();
        if incomplete_open != incomplete_close {
            tracing::warn!("Mismatched braces detected: {} '{{{{' but {} '}}}}' - template may have split expressions", incomplete_open, incomplete_close);
        }

        // Strip paragraph wrappers around block tags ({{#if}}, {{else}}, {{/if}}, etc.)
        // When a paragraph contains ONLY a block tag, remove the <w:p> wrapper so
        // Handlebars doesn't leave an empty paragraph (visible blank line) in the output.
        result = self.strip_block_tag_paragraphs(&result);

        // Special handling for table row loops
        // Move {{#each ...}} from inside table row to before <w:tr>
        // and {{/each}} from inside table row to after </w:tr>
        result = self.normalize_table_loops(&result)?;

        Ok(result)
    }

    /// Remove <w:p> wrappers from paragraphs that contain only block control tags.
    /// This prevents empty paragraphs (blank lines) when {{#if}}, {{else}}, {{/if}},
    /// {{#each}}, {{/each}}, {{#chunk}}, {{/chunk}} are placed on their own lines.
    fn strip_block_tag_paragraphs(&self, xml: &str) -> String {
        let mut result = xml.to_string();
        let mut replacements = Vec::new();

        for para_match in patterns::PARAGRAPH.find_iter(xml) {
            let para_text = para_match.as_str();

            // Extract all text content from <w:t> elements in this paragraph
            let text_parts: Vec<String> = patterns::XML_TEXT_CONTENT
                .captures_iter(para_text)
                .map(|c| c.get(1).unwrap().as_str().to_string())
                .collect();

            // Concatenate all text in the paragraph
            let full_text = text_parts.join("").trim().to_string();

            // Check if the full text is exactly one block tag
            if patterns::BLOCK_TAG.is_match(&full_text) && patterns::EXPR.is_match(&full_text) {
                // Only strip if the text is JUST the block tag (no other content)
                let stripped = patterns::EXPR.replace_all(&full_text, "");
                if stripped.trim().is_empty() {
                    replacements.push((para_match.start(), para_match.end(), full_text));
                }
            }
        }

        // Apply replacements in reverse order to preserve positions
        for (start, end, tag) in replacements.into_iter().rev() {
            result.replace_range(start..end, &tag);
        }

        result
    }

    /// Handle block tags in table rows by moving them outside of <w:tr> elements.
    /// Delegates to the shared OOXML loop normalizer parameterized on `"w:tr"`.
    fn normalize_table_loops(&self, xml: &str) -> Result<String, RenderError> {
        normalize_container_loops(xml, "w:tr")
    }

    /// Validate template syntax without data
    /// Returns a list of all syntax errors found
    pub fn validate_template_syntax(&self, template_data: &Bytes) -> Result<(), ValidationReport> {
        // Extract DOCX as ZIP archive
        let cursor = Cursor::new(template_data.to_vec());
        let mut archive = ZipArchive::new(cursor).map_err(|e| {
            ValidationReport::new_syntax(vec![ValidationIssue {
                error_type: "InvalidDocx".to_string(),
                expression: "".to_string(),
                suggestion: format!("Failed to read DOCX file: {}", e),
                severity: "error".to_string(),
                category: None,
                location: None,
            }])
        })?;

        // Identify all renderable files
        let files_to_check = self.identify_renderable_files(&mut archive).map_err(|e| {
            ValidationReport::new_syntax(vec![ValidationIssue {
                error_type: "TemplateProcessing".to_string(),
                expression: "".to_string(),
                suggestion: format!("Failed to process template: {}", e),
                severity: "error".to_string(),
                category: None,
                location: None,
            }])
        })?;

        let mut all_errors = Vec::new();

        // Check each file
        for file_name in &files_to_check {
            let xml = self
                .extract_xml_file(&mut archive, file_name)
                .map_err(|e| {
                    ValidationReport::new_syntax(vec![ValidationIssue {
                        error_type: "TemplateProcessing".to_string(),
                        expression: "".to_string(),
                        suggestion: format!("Failed to read {}: {}", file_name, e),
                        severity: "error".to_string(),
                        category: None,
                        location: Some(file_name.clone()),
                    }])
                })?;

            // Normalize XML to merge split runs
            let normalized = self.normalize_word_xml(&xml).map_err(|e| {
                ValidationReport::new_syntax(vec![ValidationIssue {
                    error_type: "TemplateProcessing".to_string(),
                    expression: "".to_string(),
                    suggestion: format!("Failed to normalize XML: {}", e),
                    severity: "error".to_string(),
                    category: None,
                    location: Some(file_name.clone()),
                }])
            })?;

            // Extract all Handlebars expressions
            let expressions: Vec<&str> = patterns::EXPR
                .find_iter(&normalized)
                .map(|m| m.as_str())
                .collect();

            // Check for broken expressions by removing all matched {{...}} and seeing
            // if any unmatched {{ remain. This avoids false positives from literal braces
            // in normal text like "Tax ({{rate}})".
            let stripped = patterns::EXPR.replace_all(&normalized, "");
            let unmatched_opens = stripped.matches("{{").count();
            if unmatched_opens > 0 {
                all_errors.push(ValidationIssue {
                    error_type: "MismatchedBraces".to_string(),
                    expression: "".to_string(),
                    suggestion: format!(
                        "Found {} broken expression(s) with '{{{{' but no matching '}}}}' — check for missing closing braces",
                        unmatched_opens
                    ),
                    severity: "error".to_string(),
                    category: None,
                    location: Some(file_name.clone()),
                });
            }

            // Validate expressions
            let mut errors = self.validate_expressions(&expressions, file_name);
            all_errors.append(&mut errors);
        }

        if all_errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationReport::new_syntax(all_errors))
        }
    }

    /// Validate a list of Handlebars expressions
    fn validate_expressions(&self, expressions: &[&str], location: &str) -> Vec<ValidationIssue> {
        let mut errors = Vec::new();
        let mut block_stack: Vec<(&str, &str)> = Vec::new(); // (block_type, expression)

        // Track what types we're currently inside
        let mut inside_each: Vec<String> = Vec::new();

        for expr in expressions {
            let trimmed = expr.trim();

            // Check for {{#chunk}} without size parameter
            if let Some(captures) = patterns::CHUNK_OPEN_FULL.captures(trimmed) {
                if captures.get(2).is_none() {
                    // Missing chunk size
                    let array_name = captures.get(1).unwrap().as_str();
                    errors.push(ValidationIssue {
                        error_type: "MissingParameter".to_string(),
                        expression: trimmed.to_string(),
                        suggestion: format!("Add chunk size: {{{{#chunk {} 2}}}}", array_name),
                        severity: "error".to_string(),
                        category: None,
                        location: Some(location.to_string()),
                    });
                }
                block_stack.push(("chunk", trimmed));
            }
            // Check for {{#each}}
            else if let Some(captures) = patterns::EACH_OPEN_FULL.captures(trimmed) {
                let array_name = captures.get(1).unwrap().as_str();

                // Check for nested identical loops
                if inside_each.contains(&array_name.to_string()) {
                    errors.push(ValidationIssue {
                        error_type: "NestedLoop".to_string(),
                        expression: format!(
                            "{{{{#each {}}}}} inside {{{{#each {}}}}}",
                            array_name, array_name
                        ),
                        suggestion: "Use {{#chunk}} for grid layouts instead of nested loops"
                            .to_string(),
                        severity: "error".to_string(),
                        category: None,
                        location: Some(location.to_string()),
                    });
                }

                inside_each.push(array_name.to_string());
                block_stack.push(("each", trimmed));
            }
            // Check for {{#if}}
            else if patterns::IF_OPEN_FULL.is_match(trimmed) {
                block_stack.push(("if", trimmed));
            }
            // Check for {{image ...}} width parameter
            else if let Some(captures) = patterns::IMAGE_HELPER.captures(trimmed) {
                for (group, name) in [(2, "width"), (4, "height")] {
                    if let Some(val) = captures.get(group) {
                        if val.as_str().parse::<u32>().is_err() {
                            errors.push(ValidationIssue {
                                error_type: "InvalidParameter".to_string(),
                                expression: trimmed.to_string(),
                                suggestion: format!(
                                    "{} must be a positive integer, got '{}'",
                                    name,
                                    val.as_str()
                                ),
                                severity: "error".to_string(),
                                category: None,
                                location: Some(location.to_string()),
                            });
                        }
                    }
                }
            }
            // Check for closing tags
            else if patterns::CHUNK_CLOSE_FULL.is_match(trimmed) {
                if let Some((block_type, _)) = block_stack.last() {
                    if *block_type != "chunk" {
                        errors.push(ValidationIssue {
                            error_type: "MismatchedClosingTag".to_string(),
                            expression: trimmed.to_string(),
                            suggestion: format!(
                                "Expected {{{{/{}}}}}, but found {{{{/chunk}}}}",
                                block_type
                            ),
                            severity: "error".to_string(),
                            category: None,
                            location: Some(location.to_string()),
                        });
                    } else {
                        block_stack.pop();
                    }
                } else {
                    errors.push(ValidationIssue {
                        error_type: "UnmatchedClosingTag".to_string(),
                        expression: trimmed.to_string(),
                        suggestion: "{{/chunk}} without matching {{#chunk}}".to_string(),
                        severity: "error".to_string(),
                        category: None,
                        location: Some(location.to_string()),
                    });
                }
            } else if patterns::EACH_CLOSE_FULL.is_match(trimmed) {
                if let Some((block_type, _)) = block_stack.last() {
                    if *block_type != "each" {
                        errors.push(ValidationIssue {
                            error_type: "MismatchedClosingTag".to_string(),
                            expression: trimmed.to_string(),
                            suggestion: format!(
                                "Expected {{{{/{}}}}}, but found {{{{/each}}}}",
                                block_type
                            ),
                            severity: "error".to_string(),
                            category: None,
                            location: Some(location.to_string()),
                        });
                    } else {
                        block_stack.pop();
                        if !inside_each.is_empty() {
                            inside_each.pop();
                        }
                    }
                } else {
                    errors.push(ValidationIssue {
                        error_type: "UnmatchedClosingTag".to_string(),
                        expression: trimmed.to_string(),
                        suggestion: "{{/each}} without matching {{#each}}".to_string(),
                        severity: "error".to_string(),
                        category: None,
                        location: Some(location.to_string()),
                    });
                }
            } else if patterns::IF_CLOSE_FULL.is_match(trimmed) {
                if let Some((block_type, _)) = block_stack.last() {
                    if *block_type != "if" {
                        errors.push(ValidationIssue {
                            error_type: "MismatchedClosingTag".to_string(),
                            expression: trimmed.to_string(),
                            suggestion: format!(
                                "Expected {{{{/{}}}}}, but found {{{{/if}}}}",
                                block_type
                            ),
                            severity: "error".to_string(),
                            category: None,
                            location: Some(location.to_string()),
                        });
                    } else {
                        block_stack.pop();
                    }
                } else {
                    errors.push(ValidationIssue {
                        error_type: "UnmatchedClosingTag".to_string(),
                        expression: trimmed.to_string(),
                        suggestion: "{{/if}} without matching {{#if}}".to_string(),
                        severity: "error".to_string(),
                        category: None,
                        location: Some(location.to_string()),
                    });
                }
            }
        }

        // Check for unclosed blocks
        for (block_type, expr) in block_stack {
            errors.push(ValidationIssue {
                error_type: "UnclosedBlock".to_string(),
                expression: expr.to_string(),
                suggestion: format!("Add closing tag: {{{{/{}}}}}", block_type),
                severity: "error".to_string(),
                category: None,
                location: Some(location.to_string()),
            });
        }

        errors
    }

    /// Parse Handlebars render errors and convert to ValidationIssue
    fn parse_render_error(
        &self,
        error: &handlebars::RenderError,
        location: &str,
    ) -> Vec<ValidationIssue> {
        let error_msg = error.to_string();
        let mut issues = Vec::new();

        // Log the full error for debugging (only last 500 chars to avoid spam)
        let log_msg = if error_msg.len() > 500 {
            format!("...{}", &error_msg[error_msg.len() - 500..])
        } else {
            error_msg.clone()
        };
        tracing::error!("Handlebars render error in {}: {}", location, log_msg);

        // Common Handlebars error patterns
        if error_msg.contains("invalid handlebars syntax") {
            // Extract line and column information from error
            // Format: "Template error: invalid handlebars syntax: <message> --> Template error in \"Unnamed\":line:column"
            let line_col = self.extract_line_column(&error_msg);
            let error_detail = self.extract_syntax_error_detail(&error_msg);

            issues.push(ValidationIssue {
                error_type: "SyntaxError".to_string(),
                expression: error_detail.unwrap_or_else(|| "Check template for unclosed {{ or mismatched tags".to_string()),
                suggestion: if let Some((line, col)) = line_col {
                    format!("Syntax error at line {}, column {}. Common causes: unclosed {{{{, missing }}}}, or Word splitting expressions across runs", line, col)
                } else {
                    "Check for unclosed tags, missing parameters, or invalid syntax. Word may have split your Handlebars expressions - try retyping them.".to_string()
                },
                severity: "error".to_string(),
                category: None,
                location: Some(location.to_string()),
            });
        } else if error_msg.contains("not found") || error_msg.contains("undefined") {
            // Missing data field
            let field_name = self
                .extract_field_name_from_error(&error_msg)
                .unwrap_or("unknown".to_string());
            issues.push(ValidationIssue {
                error_type: "MissingDataField".to_string(),
                expression: format!("{{{{{}}}}}", field_name),
                suggestion: format!("Ensure '{}' is provided in the render data", field_name),
                severity: "error".to_string(),
                category: None,
                location: Some(location.to_string()),
            });
        } else if error_msg.contains("is not iterable") || error_msg.contains("not an array") {
            // Type mismatch - trying to iterate over non-array
            let field_name = self
                .extract_field_name_from_error(&error_msg)
                .unwrap_or("unknown".to_string());
            issues.push(ValidationIssue {
                error_type: "TypeMismatch".to_string(),
                expression: format!("{{{{#each {}}}}}", field_name),
                suggestion: format!("'{}' must be an array for #each loops", field_name),
                severity: "error".to_string(),
                category: None,
                location: Some(location.to_string()),
            });
        } else {
            // Generic render error
            issues.push(ValidationIssue {
                error_type: "RenderError".to_string(),
                expression: "unknown".to_string(),
                suggestion: format!("Template rendering failed: {}", error_msg),
                severity: "error".to_string(),
                category: None,
                location: Some(location.to_string()),
            });
        }

        issues
    }

    /// Extract line and column from Handlebars error message
    fn extract_line_column(&self, error_msg: &str) -> Option<(usize, usize)> {
        // Pattern: "Unnamed":line:column
        let captures = patterns::ERROR_LINE_COL.captures(error_msg)?;

        let line = captures.get(1)?.as_str().parse().ok()?;
        let col = captures.get(2)?.as_str().parse().ok()?;

        Some((line, col))
    }

    /// Extract the actual syntax error detail from Handlebars error
    fn extract_syntax_error_detail(&self, error_msg: &str) -> Option<String> {
        // Pattern: "invalid handlebars syntax: <detail> -->"
        if let Some(start) = error_msg.find("invalid handlebars syntax:") {
            let after_prefix = &error_msg[start + 26..]; // Skip "invalid handlebars syntax:"
            if let Some(end) = after_prefix.find("-->") {
                let detail = after_prefix[..end].trim();
                return Some(detail.to_string());
            }
        }
        None
    }

    /// Extract context around an error position in the XML
    /// Shows 200 characters before and after the error position
    fn extract_error_context(&self, xml: &str, line: usize, col: usize) -> String {
        // For XML that's essentially one line, use column directly
        let pos = if line <= 2 {
            col.saturating_sub(1).min(xml.len())
        } else {
            // Multi-line: find the line and column
            let mut current_pos = 0;
            for (line_num, xml_line) in xml.lines().enumerate() {
                if line_num + 1 == line {
                    current_pos += col.saturating_sub(1).min(xml_line.len());
                    break;
                }
                current_pos += xml_line.len() + 1; // +1 for newline
            }
            current_pos.min(xml.len())
        };

        let start = pos.saturating_sub(200);
        let end = (pos + 200).min(xml.len());

        let before = &xml[start..pos];
        let after = &xml[pos..end];

        format!(
            "Context (200 chars before/after error):\n\
            ---BEFORE---\n{}\n\
            ---AT ERROR (column {})---\n{}\n\
            ---END---",
            before,
            col,
            &after[..after.len().min(50)] // Show first 50 chars after
        )
    }

    /// Extract field name from error message
    fn extract_field_name_from_error(&self, error_msg: &str) -> Option<String> {
        // Common patterns: "field 'name' not found", "'name' is not iterable", etc.
        let error_patterns: &[&Regex] = &[
            &patterns::ERROR_SINGLE_QUOTE,
            &patterns::ERROR_DOUBLE_QUOTE,
            &patterns::ERROR_FIELD,
            &patterns::ERROR_PATH,
        ];

        for re in error_patterns {
            if let Some(captures) = re.captures(error_msg) {
                if let Some(field) = captures.get(1) {
                    return Some(field.as_str().to_string());
                }
            }
        }

        None
    }

    /// Detect table cell width for automatic image sizing
    /// Looks for the containing table cell and extracts its width
    fn detect_table_cell_width(&self, xml: &str, placeholder: &str) -> Option<u32> {
        // Find the placeholder position
        let placeholder_pos = xml.find(placeholder)?;

        // Look backwards from placeholder to find the containing <w:tc> (table cell)
        let before_placeholder = &xml[..placeholder_pos];
        let tc_start = before_placeholder.rfind("<w:tc>")?;

        // Look forward from tc_start to find </w:tc> - this is our cell
        let after_tc = &xml[tc_start..];
        let tc_end_relative = after_tc.find("</w:tc>")?;
        let cell_xml = &after_tc[..tc_end_relative];

        // Extract width from <w:tcW w:w="3116" w:type="dxa"/>
        // Handle both attribute orders
        let width_match = patterns::TABLE_CELL_WIDTH.captures(cell_xml)?;
        let width_dxa: u32 = width_match.get(1)?.as_str().parse().ok()?;

        // Convert DXA (twentieths of a point) to pixels
        // 1 DXA = 1/20 point, 1 point = 1/72 inch, 1 inch = 96 pixels
        // Therefore: pixels = DXA / 20 * 1/72 * 96 = DXA / 15
        let width_px = width_dxa / 15;

        // Subtract cell padding to give images some breathing room
        // Typical Word cell padding is around 10-15 pixels
        const CELL_PADDING_PX: u32 = 20;
        let effective_width = width_px.saturating_sub(CELL_PADDING_PX);

        tracing::info!(
            "Detected table cell width: {} DXA = {} pixels (effective: {} with padding)",
            width_dxa,
            width_px,
            effective_width
        );

        Some(effective_width)
    }

    /// Resolve `___IMAGE_PLACEHOLDER___` markers into embedded image XML.
    /// Fetches images, validates them, and replaces placeholders with drawing XML
    /// Returns: (updated_files, image_files_with_rel_ids)
    async fn process_image_placeholders(
        &self,
        rendered_files: std::collections::HashMap<String, String>,
    ) -> Result<
        (
            std::collections::HashMap<String, String>,
            Vec<(String, String, String, Bytes)>,
        ),
        RenderError,
    > {
        use futures::stream::{self, StreamExt};

        /// Maximum number of images fetched/processed concurrently.
        const IMAGE_CONCURRENCY: usize = 4;

        let image_handler = ImageHandler::new()?;
        let mut embedder = ImageEmbedder::new();
        let mut image_files = Vec::new(); // (source_file, rel_id, filename, bytes)
        let mut updated_files = std::collections::HashMap::new();

        for (file_name, xml_content) in rendered_files {
            let mut updated_content = xml_content.clone();

            // Collect all image placeholders in this file
            // Pattern: ___IMAGE_PLACEHOLDER|||url|||width|||height___
            struct Placeholder {
                full_match: String,
                url: String,
                max_width: Option<u32>,
                max_height: Option<u32>,
            }

            let placeholders: Vec<Placeholder> = patterns::IMAGE_PLACEHOLDER
                .captures_iter(&xml_content)
                .map(|cap| {
                    let full_match = cap.get(0).unwrap().as_str().to_string();
                    let url = cap.get(1).unwrap().as_str().to_string();
                    let width_str = cap.get(2).unwrap().as_str();
                    let height_str = cap.get(3).unwrap().as_str();
                    let max_width = if width_str.is_empty() {
                        self.detect_table_cell_width(&xml_content, &full_match)
                    } else {
                        width_str.parse::<u32>().ok()
                    };
                    let max_height = if height_str.is_empty() {
                        None
                    } else {
                        height_str.parse::<u32>().ok()
                    };
                    Placeholder {
                        full_match,
                        url,
                        max_width,
                        max_height,
                    }
                })
                .collect();

            tracing::info!(
                "Processing file: {}, found {} image placeholders",
                file_name,
                placeholders.len()
            );

            // Fetch and process all images with bounded parallelism
            let fetch_results: Vec<Result<(Placeholder, _), RenderError>> =
                stream::iter(placeholders.into_iter().map(|p| {
                    let handler = &image_handler;
                    async move {
                        let source = ImageSource::parse(&p.url);
                        tracing::info!("Fetching image from: {} (width: {:?})", p.url, p.max_width);
                        let image_data = handler.fetch_and_process(&source, p.max_width).await?;
                        tracing::info!(
                            "Fetched image: {}x{} pixels, {} bytes",
                            image_data.width,
                            image_data.height,
                            image_data.bytes.len()
                        );
                        Ok((p, image_data))
                    }
                }))
                .buffer_unordered(IMAGE_CONCURRENCY)
                .collect()
                .await;

            // Apply results sequentially (XML replacement + embedder state must be ordered)
            for result in fetch_results {
                let (placeholder, image_data) = result?;

                // Add image to embedder and get relationship ID
                let rel_id = embedder.add_image(image_data.clone());

                // Generate drawing XML
                let drawing_xml = embedder.generate_drawing_xml(
                    &rel_id,
                    &image_data,
                    placeholder.max_width,
                    placeholder.max_height,
                );

                // Find and replace the <w:t> element containing the placeholder with the drawing
                let text_element_pattern = format!(
                    r#"<w:t[^>]*>{}<\/w:t>"#,
                    regex::escape(&placeholder.full_match)
                );
                let text_element_regex = Regex::new(&text_element_pattern).unwrap();

                if text_element_regex.is_match(&updated_content) {
                    tracing::info!("Found exact <w:t> match for placeholder in {}", file_name);
                    updated_content = text_element_regex
                        .replace(&updated_content, drawing_xml.as_str())
                        .to_string();
                } else if updated_content.contains(&placeholder.full_match) {
                    tracing::warn!(
                        "Placeholder not in <w:t> element in {}, using fallback replace",
                        file_name
                    );
                    updated_content =
                        updated_content.replace(&placeholder.full_match, &drawing_xml);
                } else {
                    tracing::error!("Placeholder not found in {} at all", file_name);
                }

                // Store image file with its source file, relationship ID, and data
                let ext = image_data.format.extension();
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                let image_filename =
                    format!("generated_{}_{}.{}", timestamp, image_files.len() + 1, ext);
                image_files.push((
                    file_name.clone(),
                    rel_id.clone(),
                    image_filename,
                    image_data.bytes.clone(),
                ));
            }

            updated_files.insert(file_name, updated_content);
        }

        Ok((updated_files, image_files))
    }

    /// Repackage the DOCX with the rendered parts and any embedded images.
    fn repackage_docx_with_files_and_images(
        &self,
        mut original_archive: ZipArchive<Cursor<Vec<u8>>>,
        mut rendered_files: std::collections::HashMap<String, String>,
        image_files: Vec<(String, String, String, Bytes)>, // (source_file, rel_id, filename, bytes)
    ) -> Result<Vec<u8>, RenderError> {
        let output_buffer = Vec::new();
        let mut zip_writer = ZipWriter::new(Cursor::new(output_buffer));

        let options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        let has_images = !image_files.is_empty();

        // Build a map of source_file -> [(rel_id, image_filename)] for relationship updates
        // Each source file (document.xml, header1.xml, etc.) gets its own .rels file
        let mut images_by_source: std::collections::HashMap<String, Vec<(String, String)>> =
            std::collections::HashMap::new();
        for (source_file, rel_id, image_filename, _) in &image_files {
            images_by_source
                .entry(source_file.clone())
                .or_default()
                .push((rel_id.clone(), image_filename.clone()));
        }

        // Determine the rels file path for a given source file
        // e.g. "word/document.xml" -> "word/_rels/document.xml.rels"
        //      "word/header1.xml"  -> "word/_rels/header1.xml.rels"
        let rels_files_needing_update: std::collections::HashMap<String, String> = images_by_source
            .keys()
            .map(|source| {
                let file_name = source.rsplit('/').next().unwrap_or(source);
                let dir = source.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
                let rels_path = format!("{}/_rels/{}.rels", dir, file_name);
                (rels_path, source.clone())
            })
            .collect();

        let id_pattern = &patterns::RELATIONSHIP_ID;

        // Track which rels files we've already written (some may not exist in the original archive)
        let mut written_rels_files = std::collections::HashSet::new();

        // Copy all files from original archive EXCEPT the ones we rendered
        for i in 0..original_archive.len() {
            let mut file = original_archive.by_index(i).map_err(|e| {
                RenderError::TemplateProcessing(format!("Failed to read archive entry: {}", e))
            })?;

            let file_name = file.name().to_string();

            // Skip files we've rendered (they'll be added later)
            if rendered_files.contains_key(&file_name) {
                continue;
            }

            // Update [Content_Types].xml to include image content types
            if has_images && file_name == "[Content_Types].xml" {
                let content_types = read_entry_capped_string(&mut file, &file_name)?;

                let updated_content_types = ensure_image_content_types(&content_types)?;

                zip_writer.start_file(&file_name, options).map_err(|e| {
                    RenderError::TemplateProcessing(format!(
                        "Failed to create file in archive: {}",
                        e
                    ))
                })?;

                zip_writer
                    .write_all(updated_content_types.as_bytes())
                    .map_err(|e| {
                        RenderError::TemplateProcessing(format!(
                            "Failed to write file to archive: {}",
                            e
                        ))
                    })?;

                continue;
            }

            // Check if this is a .rels file that needs image relationships added
            if has_images {
                if let Some(source_file) = rels_files_needing_update.get(&file_name) {
                    let rels_content = read_entry_capped_string(&mut file, &file_name)?;

                    // Find existing max ID to avoid conflicts
                    let max_id = id_pattern
                        .captures_iter(&rels_content)
                        .filter_map(|cap| {
                            cap.get(1)
                                .unwrap()
                                .as_str()
                                .strip_prefix("rId")
                                .and_then(|n| n.parse::<u32>().ok())
                        })
                        .max()
                        .unwrap_or(0);

                    // Remap IDs and update rendered file content
                    let image_entries = images_by_source.get(source_file).unwrap();
                    let mut image_rels = Vec::new();

                    for (new_rel_id, (old_rel_id, image_filename)) in
                        (max_id + 1..).zip(image_entries.iter())
                    {
                        let new_id = format!("rId{}", new_rel_id);
                        tracing::info!(
                            "Mapping {} -> {} for {} in {}",
                            old_rel_id,
                            new_id,
                            image_filename,
                            file_name
                        );

                        // Update the rendered XML to use the new ID
                        if old_rel_id != &new_id {
                            if let Some(content) = rendered_files.get_mut(source_file) {
                                if content.contains(old_rel_id.as_str()) {
                                    *content = content.replace(old_rel_id, &new_id);
                                }
                            }
                        }

                        image_rels.push((new_id, image_filename.clone()));
                    }

                    let embedder = ImageEmbedder::new();
                    let updated_rels = embedder.update_relationships(&rels_content, &image_rels)?;

                    zip_writer.start_file(&file_name, options).map_err(|e| {
                        RenderError::TemplateProcessing(format!(
                            "Failed to create file in archive: {}",
                            e
                        ))
                    })?;

                    zip_writer.write_all(updated_rels.as_bytes()).map_err(|e| {
                        RenderError::TemplateProcessing(format!(
                            "Failed to write file to archive: {}",
                            e
                        ))
                    })?;

                    written_rels_files.insert(file_name);
                    continue;
                }
            }

            // Start new file in output archive
            zip_writer.start_file(&file_name, options).map_err(|e| {
                RenderError::TemplateProcessing(format!("Failed to create file in archive: {}", e))
            })?;

            // Copy file contents
            let contents = read_entry_capped(&mut file, &file_name)?;

            zip_writer.write_all(&contents).map_err(|e| {
                RenderError::TemplateProcessing(format!("Failed to write file to archive: {}", e))
            })?;
        }

        // Create .rels files that didn't exist in the original archive
        // (e.g. header1.xml.rels when the header had no prior relationships)
        for (rels_path, source_file) in &rels_files_needing_update {
            if written_rels_files.contains(rels_path.as_str()) {
                continue;
            }

            tracing::info!("Creating new rels file {} for {}", rels_path, source_file);

            let empty_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#;

            let image_entries = images_by_source.get(source_file).unwrap();
            let mut image_rels = Vec::new();

            for (new_rel_id, (old_rel_id, image_filename)) in (1u32..).zip(image_entries.iter()) {
                let new_id = format!("rId{}", new_rel_id);

                if old_rel_id != &new_id {
                    if let Some(content) = rendered_files.get_mut(source_file) {
                        if content.contains(old_rel_id.as_str()) {
                            *content = content.replace(old_rel_id, &new_id);
                        }
                    }
                }

                image_rels.push((new_id, image_filename.clone()));
            }

            let embedder = ImageEmbedder::new();
            let updated_rels = embedder.update_relationships(empty_rels, &image_rels)?;

            zip_writer.start_file(rels_path, options).map_err(|e| {
                RenderError::TemplateProcessing(format!("Failed to create file in archive: {}", e))
            })?;

            zip_writer.write_all(updated_rels.as_bytes()).map_err(|e| {
                RenderError::TemplateProcessing(format!("Failed to write file to archive: {}", e))
            })?;
        }

        // Add all rendered files (document.xml, headers, footers)
        for (file_name, rendered_xml) in rendered_files {
            zip_writer.start_file(&file_name, options).map_err(|e| {
                RenderError::TemplateProcessing(format!("Failed to add {}: {}", file_name, e))
            })?;

            zip_writer.write_all(rendered_xml.as_bytes()).map_err(|e| {
                RenderError::TemplateProcessing(format!("Failed to write {}: {}", file_name, e))
            })?;
        }

        // Add image files to word/media directory
        for (_, _, filename, image_data) in image_files {
            let media_path = format!("word/media/{}", filename);

            tracing::info!(
                "Writing image file: {} ({} bytes)",
                media_path,
                image_data.len()
            );

            zip_writer.start_file(&media_path, options).map_err(|e| {
                RenderError::TemplateProcessing(format!("Failed to add image {}: {}", filename, e))
            })?;

            zip_writer.write_all(&image_data).map_err(|e| {
                RenderError::TemplateProcessing(format!(
                    "Failed to write image {}: {}",
                    filename, e
                ))
            })?;
        }

        // Finalize the ZIP and extract the bytes
        let cursor = zip_writer.finish().map_err(|e| {
            RenderError::TemplateProcessing(format!("Failed to finalize DOCX archive: {}", e))
        })?;

        Ok(cursor.into_inner())
    }
}

impl Default for DocxTemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}
