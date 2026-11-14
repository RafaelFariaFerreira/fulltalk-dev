//! Runs the whole delivery path (focus probe, clipboard, paste keystroke)
//! against whatever has focus, without recording anything.
//!
//! ```sh
//! cargo run -p fulltalk-platform --example deliver -- "texto de teste" 3
//! ```
//!
//! The second argument is how many seconds to wait first, so there is time to
//! click on the window to test.

use std::time::Duration;

use fulltalk_core::delivery::{self, Platform};
use fulltalk_platform::clipboard::SystemClipboard;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let text = args.next().unwrap_or_else(|| "fulltalk dev".to_owned());
    let wait: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);

    let mut clipboard = SystemClipboard::new()?;
    let focus = fulltalk_platform::focus::system_probe();
    let mut paster = fulltalk_platform::paste::system_paster()?;
    std::thread::sleep(Duration::from_secs(wait));

    println!("focus: {:?}", focus.focused());
    let outcome = delivery::deliver(
        &text,
        &mut clipboard,
        focus.as_ref(),
        paster.as_mut(),
        Platform::current(),
        true,
    )?;
    println!("outcome: {outcome:?}");
    // On X11 the clipboard is served by this process: give the paste time to read it.
    std::thread::sleep(Duration::from_millis(800));
    Ok(())
}
