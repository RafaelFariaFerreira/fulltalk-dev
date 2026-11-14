//! Prints what has keyboard focus every second — switch windows to try it.
//!
//! ```sh
//! cargo run -p fulltalk-platform --example probe_focus
//! ```

use std::time::{Duration, Instant};

fn main() {
    let probe = fulltalk_platform::focus::system_probe();
    for _ in 0..10 {
        let t = Instant::now();
        let target = probe.focused();
        println!("{target:?}  ({} ms)", t.elapsed().as_millis());
        std::thread::sleep(Duration::from_secs(1));
    }
}
