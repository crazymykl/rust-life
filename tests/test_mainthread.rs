// This needs to be run with nextest. Each example inits an EventLoop
// which can only happen once, only on the main thread (on macOS)
// libtest-mimic will use the main thread when test_threads == 1

use std::panic::catch_unwind;

use libtest_mimic::{run, Arguments, Trial};
use rust_life::EXAMPLES;

fn main() {
    let args = Arguments {
        test_threads: Some(1),
        ..Arguments::from_args()
    };

    run(
        &args,
        EXAMPLES
            .into_iter()
            .map(|(name, func)| {
                Trial::test(*name, move || {
                    catch_unwind(func).map_err(|x| format!("{x:?}").into())
                })
            })
            .collect(),
    )
    .exit();
}
