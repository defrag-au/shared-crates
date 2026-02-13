//! Compile-time SCSS to CSS compilation macros.
//!
//! This crate provides procedural macros for compiling SCSS to CSS at compile time,
//! resulting in zero runtime overhead for stylesheet processing.
//!
//! ## `scss!` macro
//!
//! Compiles an SCSS file to CSS at compile time. The path is relative to the
//! crate's `Cargo.toml` directory.
//!
//! ```ignore
//! use scss_macros::scss;
//!
//! const STYLES: &str = scss!("src/components/my_component.scss");
//! ```
//!
//! Features:
//! - Full SCSS syntax support (variables, nesting, mixins, etc.)
//! - Compile-time compilation - no runtime overhead
//! - Errors surface as compile errors with file/line info
//! - Compressed output for smaller bundle sizes
//!
//! ## `scss_inline!` macro
//!
//! Compiles inline SCSS strings at compile time. Useful for small component styles.
//!
//! ```ignore
//! use scss_macros::scss_inline;
//!
//! const STYLES: &str = scss_inline!(r#"
//!     .container {
//!         display: flex;
//!         .item {
//!             flex: 1;
//!             &:hover { background: blue; }
//!         }
//!     }
//! "#);
//! ```

use proc_macro::TokenStream;
use quote::quote;
use std::path::PathBuf;
use syn::{parse_macro_input, LitStr};

/// Compile an SCSS file to CSS at compile time.
///
/// The path is relative to the crate's `Cargo.toml` directory (i.e., `CARGO_MANIFEST_DIR`).
///
/// # Example
///
/// ```ignore
/// use scss_macros::scss;
///
/// const STYLES: &str = scss!("src/styles/component.scss");
/// ```
///
/// # Errors
///
/// - Compile error if the file doesn't exist
/// - Compile error if the SCSS is invalid
#[proc_macro]
pub fn scss(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    let relative_path = path_lit.value();

    // Get the manifest directory of the crate using this macro
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let full_path = PathBuf::from(&manifest_dir).join(&relative_path);

    // Check file exists
    if !full_path.exists() {
        let err_msg = format!("SCSS file not found: {}", full_path.display());
        return syn::Error::new_spanned(&path_lit, err_msg)
            .to_compile_error()
            .into();
    }

    // Read the file
    let scss_content = match std::fs::read_to_string(&full_path) {
        Ok(content) => content,
        Err(e) => {
            let err_msg = format!("Failed to read SCSS file '{}': {}", full_path.display(), e);
            return syn::Error::new_spanned(&path_lit, err_msg)
                .to_compile_error()
                .into();
        }
    };

    // Configure grass options - add both the manifest dir and the file's parent dir as load paths
    let file_dir = full_path.parent().unwrap_or(&full_path);
    let options = grass::Options::default()
        .style(grass::OutputStyle::Compressed)
        .load_path(&manifest_dir)
        .load_path(file_dir);

    // Compile SCSS to CSS
    let css = match grass::from_string(scss_content, &options) {
        Ok(css) => css,
        Err(e) => {
            let err_msg = format!("SCSS compilation error in '{}': {}", relative_path, e);
            return syn::Error::new_spanned(&path_lit, err_msg)
                .to_compile_error()
                .into();
        }
    };

    // Return the CSS as a string literal
    let expanded = quote! {
        #css
    };

    expanded.into()
}

/// Compile inline SCSS to CSS at compile time.
///
/// Useful for small amounts of component-specific styles that don't warrant
/// a separate file.
///
/// # Example
///
/// ```ignore
/// use scss_macros::scss_inline;
///
/// const STYLES: &str = scss_inline!(r#"
///     .container {
///         display: flex;
///         .item {
///             flex: 1;
///             &:hover { background: blue; }
///         }
///     }
/// "#);
/// ```
#[proc_macro]
pub fn scss_inline(input: TokenStream) -> TokenStream {
    let scss_lit = parse_macro_input!(input as LitStr);
    let scss_content = scss_lit.value();

    // Configure grass options
    let options = grass::Options::default().style(grass::OutputStyle::Compressed);

    // Compile SCSS to CSS
    let css = match grass::from_string(scss_content, &options) {
        Ok(css) => css,
        Err(e) => {
            let err_msg = format!("SCSS compilation error: {}", e);
            return syn::Error::new_spanned(&scss_lit, err_msg)
                .to_compile_error()
                .into();
        }
    };

    // Return the CSS as a string literal
    let expanded = quote! {
        #css
    };

    expanded.into()
}
