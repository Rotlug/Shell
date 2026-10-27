//! Compiles the styles and says how it went: `HOME=$(mktemp -d) cargo run --example compile`
use std::time::Instant;

fn main() {
    let start = Instant::now();

    match gms_style::compile() {
        Ok(css) => println!(
            "{} bytes of css in {:?}, foreground-color-primary: {:?}",
            css.len(),
            start.elapsed(),
            gms_style::get_color("foreground-color-primary"),
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
