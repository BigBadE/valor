//! WGPU backend implementation as a standalone crate within the renderer module.
//! This crate contains all WGPU-specific rendering code and depends on the renderer crate
//! for backend-agnostic types like DisplayList and DrawText.
#![allow(
    let_underscore_drop,
    clippy::needless_raw_strings,
    clippy::needless_raw_string_hashes,
    clippy::field_scoped_visibility_modifiers,
    clippy::missing_docs_in_private_items,
    clippy::missing_inline_in_public_items,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::min_ident_chars,
    clippy::similar_names,
    clippy::single_char_lifetime_names,
    clippy::module_name_repetitions,
    clippy::many_single_char_names,
    clippy::allow_attributes_without_reason,
    clippy::doc_markdown,
    clippy::trivially_copy_pass_by_ref,
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::needless_pass_by_value,
    clippy::absolute_paths,
    clippy::missing_const_for_fn,
    clippy::unused_trait_names,
    clippy::std_instead_of_core,
    clippy::default_trait_access,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::expect_used,
    clippy::cast_possible_wrap,
    clippy::let_underscore_must_use,
    clippy::let_underscore_untyped,
    clippy::explicit_into_iter_loop,
    clippy::explicit_iter_loop,
    clippy::multiple_inherent_impl,
    clippy::implicit_hasher,
    clippy::std_instead_of_core,
    clippy::unwrap_used,
    clippy::map_unwrap_or,
    clippy::unused_self,
    clippy::expect_used,
    clippy::manual_let_else,
    clippy::unnecessary_wraps,
    clippy::cognitive_complexity,
    clippy::semicolon_outside_block,
    clippy::cast_lossless,
    clippy::shadow_unrelated,
    clippy::string_add,
    clippy::if_not_else,
    clippy::str_to_string,
    clippy::default_trait_access,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::semicolon_if_nothing_returned,
    clippy::default_numeric_fallback,
    clippy::cast_possible_wrap,
    clippy::or_fun_call,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::integer_division_remainder_used,
    clippy::integer_division,
    clippy::explicit_iter_loop,
    clippy::ref_option,
    clippy::cloned_instead_of_copied,
    clippy::std_instead_of_core,
    clippy::duplicated_attributes,
    clippy::if_then_some_else_none,
    clippy::redundant_pub_crate,
    clippy::match_wildcard_for_single_variants,
    clippy::option_if_let_else,
    clippy::use_self,
    clippy::clone_on_ref_ptr,
    clippy::unwrap_in_result,
    clippy::missing_asserts_for_indexing,
    reason = "WGPU backend uses GPU-specific patterns and shader strings"
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
