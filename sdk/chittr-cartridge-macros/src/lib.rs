//! `#[hook]`: exports a Chittr Server Guest Hook with JSON in and out.
//!
//! The annotated function is the Hook. Its name is the export name the host calls (`init`,
//! `render`, ...), its single argument is the input envelope, and its return value is serialized
//! as the output. A return type of `Result<T, HookError>` reports handled failures as
//! `{ "error": { code, message } }`.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, FnArg, ItemFn, PathArguments, ReturnType, Type};

const HOOK_NAMES: &[&str] = &[
    "init",
    "render",
    "migrate",
    "on_control",
    "on_reply",
    "on_reaction",
    "on_rechitt",
    "on_view",
    "on_timer",
    "on_canvas_message",
    "on_file",
    "render_config",
];

#[proc_macro_attribute]
pub fn hook(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return error(
            proc_macro2::Span::call_site(),
            "#[hook] takes no arguments; name the function after its Hook",
        );
    }
    let function = parse_macro_input!(item as ItemFn);
    let name = function.sig.ident.to_string();
    let span = function.sig.ident.span();

    if !HOOK_NAMES.contains(&name.as_str()) {
        return error(
            span,
            &format!(
                "`{name}` is not a Hook. Expected one of: {}",
                HOOK_NAMES.join(", ")
            ),
        );
    }
    if function.sig.inputs.len() != 1 || !matches!(function.sig.inputs[0], FnArg::Typed(_)) {
        return error(
            span,
            "a Hook takes exactly one argument: its input envelope",
        );
    }
    let ReturnType::Type(_, return_type) = &function.sig.output else {
        return error(span, "a Hook must return its output envelope");
    };
    let runner = if is_result(return_type) {
        format_ident!("run_fallible")
    } else {
        format_ident!("run")
    };
    let ident = &function.sig.ident;
    let export = format_ident!("__chittr_export_{}", name);

    quote! {
        #function

        #[doc(hidden)]
        #[export_name = #name]
        pub extern "C" fn #export() -> i32 {
            ::chittr_cartridge::__private::#runner(#ident)
        }
    }
    .into()
}

/// True when the return type is spelled `Result<...>`, the fallible form of a Hook.
fn is_result(ty: &Type) -> bool {
    let Type::Path(path) = ty else { return false };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    segment.ident == "Result" && matches!(segment.arguments, PathArguments::AngleBracketed(_))
}

fn error(span: proc_macro2::Span, message: &str) -> TokenStream {
    syn::Error::new(span, message).to_compile_error().into()
}
