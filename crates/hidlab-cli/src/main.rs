//! `hid-lab` command-line tool.

mod args;
mod devices;
mod features;
mod output;

use std::process::ExitCode;

use args::Command;

fn main() -> ExitCode {
    let command = match args::parse(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::from(2);
        }
    };
    let result = match command {
        Command::Help(topic) => {
            print!("{}", topic.text());
            Ok(())
        }
        Command::Version => {
            println!("hid-lab {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Devices(a) => devices::run(&a),
        Command::Features(a) => features::run(&a),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}
