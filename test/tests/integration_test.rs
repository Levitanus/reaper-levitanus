use std::env;

use rea_rs_test::{run_integration_test, ReaperVersion};

#[test]
fn main() {
    env::set_var(
        "RUST_LOG",
        "info,reaper_levitanus=trace,rea_rs=debug,reaper_test_extension_plugin=trace",
    );
    run_integration_test(ReaperVersion::latest());
}
