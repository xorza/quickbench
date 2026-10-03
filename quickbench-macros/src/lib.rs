//! Proc-macros for bench crate.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::meta::{self, ParseNestedMeta};
use syn::parse::Parser;
use syn::{FnArg, ItemFn, LitBool, LitInt, Pat};

/// The `#[quick_bench(..)]` arguments. A limit left unset keeps the `Bencher` default.
#[derive(Debug, Default, PartialEq, Eq)]
struct QuickBenchArgs {
    warmup_time_ms: Option<u64>,
    bench_time_ms: Option<u64>,
    warmup_iters: Option<u64>,
    iters: Option<u64>,
    ignore: Option<bool>,
}

impl QuickBenchArgs {
    /// Parse `key = value` pairs, comma-separated. A key it does not know, or one given twice,
    /// is an error rather than a setting silently dropped.
    fn parse(attr: TokenStream2) -> syn::Result<Self> {
        let mut args = Self::default();
        meta::parser(|meta| args.set(&meta)).parse2(attr)?;
        Ok(args)
    }

    fn set(&mut self, meta: &ParseNestedMeta<'_>) -> syn::Result<()> {
        let key = meta.path.require_ident()?.to_string();
        let fresh = match key.as_str() {
            "warmup_time_ms" => fill(&mut self.warmup_time_ms, int(meta)?),
            "bench_time_ms" => fill(&mut self.bench_time_ms, int(meta)?),
            "warmup_iters" => fill(&mut self.warmup_iters, int(meta)?),
            "iters" => fill(&mut self.iters, int(meta)?),
            "ignore" => fill(&mut self.ignore, meta.value()?.parse::<LitBool>()?.value()),
            _ => {
                return Err(meta.error(format!(
                    "unknown quick_bench attribute: `{key}` (expected: warmup_time_ms, bench_time_ms, warmup_iters, iters, ignore)"
                )));
            }
        };
        if fresh {
            Ok(())
        } else {
            Err(meta.error(format!("quick_bench attribute `{key}` is given twice")))
        }
    }
}

/// Store `value` in an empty `slot`. False when the slot already held one.
fn fill<T>(slot: &mut Option<T>, value: T) -> bool {
    if slot.is_some() {
        return false;
    }
    *slot = Some(value);
    true
}

fn int(meta: &ParseNestedMeta<'_>) -> syn::Result<u64> {
    meta.value()?.parse::<LitInt>()?.base10_parse()
}

/// Attribute macro for creating benchmark tests.
///
/// # Usage
///
/// ```ignore
/// use quickbench::quick_bench;
/// use quickbench::Bencher;
///
/// #[quick_bench]
/// fn bench_something(b: Bencher) {
///     b.bench(|| {
///         // code to benchmark
///     });
/// }
///
/// // Time-based: runs for specified duration
/// #[quick_bench(warmup_time_ms = 500, bench_time_ms = 2000)]
/// fn bench_time_based(b: Bencher) {
///     b.bench(|| {
///         // code to benchmark
///     });
/// }
///
/// // Iteration-based: runs exact number of iterations
/// #[quick_bench(warmup_iters = 100, iters = 1000)]
/// fn bench_iteration_based(b: Bencher) {
///     b.bench(|| {
///         // code to benchmark
///     });
/// }
///
/// // Combined: stops at whichever limit is reached first
/// #[quick_bench(warmup_time_ms = 500, warmup_iters = 100, bench_time_ms = 2000, iters = 500)]
/// fn bench_combined(b: Bencher) {
///     b.bench(|| {
///         // code to benchmark
///     });
/// }
///
/// // Run as a normal test (not ignored)
/// #[quick_bench(ignore = false)]
/// fn bench_not_ignored(b: Bencher) {
///     b.bench(|| {
///         // code to benchmark
///     });
/// }
/// ```
#[proc_macro_attribute]
pub fn quick_bench(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// The `#[test]` function that `#[quick_bench(attr)]` makes of `item`.
fn expand(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args = QuickBenchArgs::parse(attr)?;
    let input = syn::parse2::<ItemFn>(item)?;

    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let fn_body = &input.block;
    let fn_vis = &input.vis;

    if input.sig.inputs.len() != 1 {
        return Err(syn::Error::new_spanned(
            &input.sig.inputs,
            "quick_bench function must have exactly one parameter: `b: Bencher`",
        ));
    }
    let (param_name, param_ty) = match &input.sig.inputs[0] {
        FnArg::Typed(pat_type) => match &*pat_type.pat {
            Pat::Ident(pat_ident) => (&pat_ident.ident, &*pat_type.ty),
            _ => {
                return Err(syn::Error::new_spanned(
                    &pat_type.pat,
                    "parameter must be a simple identifier",
                ));
            }
        },
        FnArg::Receiver(_) => {
            return Err(syn::Error::new_spanned(
                &input.sig.inputs[0],
                "quick_bench function cannot have self parameter",
            ));
        }
    };

    let ignore_attr = if args.ignore.unwrap_or(true) {
        quote! { #[cfg_attr(test, ignore = "a quickbench bench; run it with --ignored")] }
    } else {
        quote! {}
    };

    // A phase given only an iteration count runs exactly that many: the default time limit
    // would otherwise still stop it first.
    let disable_warmup_time = if args.warmup_iters.is_some() && args.warmup_time_ms.is_none() {
        quote! { .without_warmup_time() }
    } else {
        quote! {}
    };
    let disable_bench_time = if args.iters.is_some() && args.bench_time_ms.is_none() {
        quote! { .without_bench_time() }
    } else {
        quote! {}
    };

    let warmup_time_call = args
        .warmup_time_ms
        .map(|ms| quote! { .with_warmup_time_ms(#ms) });
    let bench_time_call = args
        .bench_time_ms
        .map(|ms| quote! { .with_bench_time_ms(#ms) });
    let warmup_iters_call = args
        .warmup_iters
        .map(|iters| quote! { .with_warmup_iters(#iters) });
    let iters_call = args.iters.map(|iters| quote! { .with_iters(#iters) });

    Ok(quote! {
        #[cfg_attr(test, test)]
        #ignore_attr
        #fn_vis fn #fn_name() {
            let #param_name: #param_ty = ::quickbench::Bencher::new(#fn_name_str)
                #disable_warmup_time
                #disable_bench_time
                #warmup_time_call
                #bench_time_call
                #warmup_iters_call
                #iters_call
                .with_lock_name(env!("CARGO_PKG_NAME"))
                .with_output_dir(env!("CARGO_MANIFEST_DIR"));
            #fn_body
        }
    })
}

#[cfg(test)]
mod tests;
