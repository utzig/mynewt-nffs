//! Core tests
//!
//! Run the existing testsuite as a Rust unit test.

extern crate fs_sim;

use fs_sim::{Run, testlog};

macro_rules! sim_test {
    ($name:ident, $maker:ident, $test:ident) => {
        #[test]
        fn $name() {
            testlog::setup();

            Run::each_device(|r| {
                let fs = r.$maker();
                assert!(!fs.$test());
            });
        }
    };
}

sim_test!(basic, make_fs, run_basic);
