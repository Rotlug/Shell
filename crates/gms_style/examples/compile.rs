//! Compiles a stylesheet (`main` unless told otherwise) and says how it went:
//! `cargo run --example compile [entry]`
use std::time::Instant;

fn main() {
    let start = Instant::now();

    let entry = std::env::args().nth(1).unwrap_or_else(|| "main".to_string());

    match gms_style::compile(&entry) {
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
