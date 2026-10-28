#[macro_use(clone)]
extern crate glib;

mod cli;
mod entry;
mod matching;
mod window;

use std::cell::RefCell;
use std::io::{IsTerminal as _, stdin};
use std::process::ExitCode;
use std::rc::Rc;

use gtk::prelude::*;

use self::cli::{Parsed, USAGE};

fn main() -> ExitCode {
    let options = match cli::parse(std::env::args().skip(1)) {
        Ok(Parsed::Run(options)) => options,
        Ok(Parsed::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("gms-launcher: {error}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    if stdin().is_terminal() {
        eprintln!("gms-launcher: the entries are read from stdin\n\n{USAGE}");
        return ExitCode::from(2);
    }

    let entries = entry::read(stdin().lock());
    if entries.is_empty() {
        eprintln!("gms-launcher: no entries");
        return ExitCode::FAILURE;
    }

    let picked = Rc::new(RefCell::new(None));

    let application = gtk::Application::builder()
        .application_id("sn.inpr.gms_launcher")
        // The entries come from stdin, which a first instance couldn't be given by a second
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    // The theme needs GTK to be ready, which it is once the application starts
    application.connect_startup(|_| {
        gms_ui::style::load("launcher");
    });

    application.connect_activate(clone!(
        #[strong] picked,
        move |application| window::present(application, entries.clone(), &options.prompt, picked.clone())
    ));

    // Its own arguments are not for GTK
    application.run_with_args(&["gms-launcher"]);

    match picked.take() {
        Some(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        None => ExitCode::FAILURE,
    }
}
