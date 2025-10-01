//! WGPU backend implementation as a standalone crate within the renderer module.
//! This crate contains all WGPU-specific rendering code and depends on the renderer crate
//! for backend-agnostic types like DisplayList and DrawText.
#![allow(
    clippy::min_ident_chars,
    clippy::integer_division,
    clippy::integer_division_remainder_used,
    clippy::missing_docs_in_private_items,
    clippy::missing_inline_in_public_items,
    clippy::multiple_inherent_impl,
    clippy::absolute_paths,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::default_numeric_fallback,
    clippy::too_many_lines,
    clippy::explicit_iter_loop,
    clippy::missing_const_for_fn,
    clippy::redundant_pub_crate,
    clippy::let_underscore_must_use,
    clippy::let_underscore_untyped,
    clippy::match_wildcard_for_single_variants,
    clippy::std_instead_of_core,
    clippy::option_if_let_else,
    clippy::cloned_instead_of_copied,
    clippy::unwrap_in_result,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::missing_asserts_for_indexing,
    clippy::semicolon_outside_block,
    clippy::shadow_unrelated,
    clippy::if_then_some_else_none,
    clippy::missing_panics_doc,
    clippy::use_self,
    clippy::default_trait_access,
    clippy::clone_on_ref_ptr,
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::unnecessary_wraps,
    clippy::cognitive_complexity,
    clippy::suboptimal_flops,
    clippy::explicit_into_iter_loop,
    clippy::semicolon_if_nothing_returned,
    clippy::unused_self,
    clippy::field_scoped_visibility_modifiers,
    clippy::allow_attributes_without_reason,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::unused_trait_names,
    clippy::trivially_copy_pass_by_ref,
    clippy::map_unwrap_or,
    clippy::too_many_arguments,
    clippy::needless_raw_strings,
    clippy::needless_raw_string_hashes,
    reason = "GPU backend code uses short names for coordinates, integer math for pixel calculations, numeric casts for graphics operations, expect/unwrap for internal invariants, and WGPU-specific patterns"
)]

mod error;
mod logical_encoder;
mod offscreen;
mod pipelines;
pub mod state;
mod text;
mod texture_pool;

pub use error::{submit_with_validation, with_validation_scope};
pub use logical_encoder::LogicalEncoder;
pub use offscreen::render_display_list_to_rgba;
pub use pipelines::{Vertex, build_pipeline_and_buffers, build_texture_pipeline};
pub use state::{Layer, RenderState};
pub use texture_pool::TexturePool;
