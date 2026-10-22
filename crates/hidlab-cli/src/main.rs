//! `hid-lab` command-line tool.

mod args;
mod devices;
mod features;
mod output;
mod read;
mod target;
mod watch;

use std::process::ExitCode;

use args::Command;

/// Restores the default SIGPIPE action so `hid-lab ... | head` ends quietly
/// instead of panicking inside `println!` (Rust ignores SIGPIPE by default).
fn restore_sigpipe() {
    // SAFETY: called once at startup before any thread is spawned; setting a
    // signal disposition to SIG_DFL has no memory-safety requirements.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

fn main() -> ExitCode {
    restore_sigpipe();
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
        Command::Read(a) => read::run(&a),
        Command::Watch(a) => watch::run(&a),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}
