use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::expand;

fn bench_fn() -> TokenStream2 {
    quote! { fn bench_x(b: Bencher) { b.bench(work); } }
}

/// What `bench_fn` expands to, with `attrs` after `#[test]` and `calls` on the builder.
fn expanded(attrs: &TokenStream2, calls: &TokenStream2) -> String {
    quote! {
        #[cfg_attr(test, test)]
        #attrs
        fn bench_x() {
            let b: Bencher = ::quickbench::Bencher::new("bench_x")
                #calls
                .with_lock_name(env!("CARGO_PKG_NAME"))
                .with_output_dir(env!("CARGO_MANIFEST_DIR"));
            { b.bench(work); }
        }
    }
    .to_string()
}

/// Each argument becomes its builder call, a bench is ignored unless it says otherwise, and a
/// phase given only an iteration count loses its time limit.
#[test]
fn each_argument_becomes_its_builder_call() {
    let ignored =
        quote! { #[cfg_attr(test, ignore = "a quickbench bench; run it with --ignored")] };
    let none = quote! {};
    for (attr, attrs, calls) in [
        (quote! {}, &ignored, quote! {}),
        (
            quote! { warmup_iters = 1, iters = 3 },
            &ignored,
            quote! {
                .without_warmup_time()
                .without_bench_time()
                .with_warmup_iters(1u64)
                .with_iters(3u64)
            },
        ),
        (
            quote! { warmup_time_ms = 5, warmup_iters = 1, bench_time_ms = 9, iters = 3, },
            &ignored,
            quote! {
                .with_warmup_time_ms(5u64)
                .with_bench_time_ms(9u64)
                .with_warmup_iters(1u64)
                .with_iters(3u64)
            },
        ),
        (
            quote! { iters = 3, ignore = false },
            &none,
            quote! { .without_bench_time() .with_iters(3u64) },
        ),
        (
            quote! { bench_time_ms = 100, ignore = true },
            &ignored,
            quote! { .with_bench_time_ms(100u64) },
        ),
    ] {
        let got = expand(attr.clone(), bench_fn()).unwrap().to_string();
        assert_eq!(got, expanded(attrs, &calls), "#[quick_bench({attr})]");
    }
}

#[test]
fn a_malformed_attribute_or_function_is_refused() {
    let one_param = "quick_bench function must have exactly one parameter: `b: Bencher`";
    for (attr, item, message) in [
        (
            quote! { iter = 3 },
            bench_fn(),
            "unknown quick_bench attribute: `iter` (expected: warmup_time_ms, bench_time_ms, warmup_iters, iters, ignore)",
        ),
        (
            quote! { iters = 3, iters = 4 },
            bench_fn(),
            "quick_bench attribute `iters` is given twice",
        ),
        (
            quote! { iters = 3 ignore = false },
            bench_fn(),
            "expected `,`",
        ),
        (
            quote! { iters = "3" },
            bench_fn(),
            "expected integer literal",
        ),
        (
            quote! { ignore = 1 },
            bench_fn(),
            "expected boolean literal",
        ),
        (quote! {}, quote! { fn f() {} }, one_param),
        (quote! {}, quote! { fn f(a: Bencher, c: u8) {} }, one_param),
        (
            quote! {},
            quote! { fn f(&self) {} },
            "quick_bench function cannot have self parameter",
        ),
        (
            quote! {},
            quote! { fn f((a, c): (Bencher, u8)) {} },
            "parameter must be a simple identifier",
        ),
    ] {
        let error = expand(attr.clone(), item.clone()).unwrap_err();
        assert_eq!(error.to_string(), message, "#[quick_bench({attr})] {item}");
    }
}
