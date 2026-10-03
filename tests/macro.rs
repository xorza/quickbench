use quickbench::{Bencher, quick_bench};

/// The attribute makes a test that runs, under the function's name, exactly the iterations it
/// names: one warmup, then three measured.
#[quick_bench(ignore = false, warmup_iters = 1, iters = 3)]
fn the_attribute_runs_the_iterations_it_names(b: Bencher) {
    let dir = tempfile::tempdir().unwrap();
    let mut calls = 0;
    let result = b.with_output_dir(dir.path()).bench(|| calls += 1);
    assert_eq!(result.name(), "the_attribute_runs_the_iterations_it_names");
    assert_eq!((calls, result.iterations()), (1 + 3, 3));
}
