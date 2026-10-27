//! `brightness` prints the monitors and what happens to them.
//! `brightness set <id> <percent>` sets the brightness of one and exits.
use std::time::Duration;

use brightness::{Brightness, event::Event};

fn main() -> brightness::Result<()> {
    let (brightness, mut rx) = Brightness::new()?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let [command, id, percent] = args.as_slice()
        && command == "set"
        && let Ok(percent) = percent.parse::<f64>()
    {
        while let Some(event) = rx.blocking_recv() {
            if let Event::Added(monitor) = &event
                && monitor.id.as_str() == id
            {
                brightness.set_brightness(&monitor.id, percent / 100.0);
                // Give it a moment to happen
                std::thread::sleep(Duration::from_millis(600));
                println!("set {}", monitor.id);
                return Ok(());
            }
        }

        return Ok(());
    }

    while let Some(event) = rx.blocking_recv() {
        println!("{event:?}");
    }

    Ok(())
}
