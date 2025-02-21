use proc_macro::TokenStream;

use digest::Digest;
use quote::{format_ident, quote};
use syn::{parse_macro_input, parse_quote, FnArg, Ident, ItemFn, PatIdent, ReturnType, Signature};

#[proc_macro_attribute]
pub fn hotreload_start(args: TokenStream, input: TokenStream) -> TokenStream {
    // let module_ident = parse_macro_input!(args as Ident);
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = parse_macro_input!(input as ItemFn);

    let mut outer_sig = sig.clone();
    for (idx, arg) in outer_sig.inputs.iter_mut().enumerate() {
        if let FnArg::Typed(arg) = arg {
            arg.pat = Box::new(syn::Pat::Ident(PatIdent {
                attrs: Vec::new(),
                by_ref: None,
                mutability: None,
                ident: format_ident!("arg{}", idx),
                subpat: None,
            }))
        }
    }

    let mut inner_sig = sig.clone();
    inner_sig.ident = format_ident!("__hotreload_start_{}", sig.ident);
    let inner_fn_name = inner_sig.ident.clone();
    let inner_fn_name_str = inner_sig.ident.clone().to_string();

    quote! {
        #(#attrs)*
        #vis #outer_sig {
            #[no_mangle]
            #[inline(never)]
            #inner_sig {
                #block
            }

            use_hotreload_component(#inner_fn_name_str, #inner_fn_name)
        }
    }
    .into()
}
