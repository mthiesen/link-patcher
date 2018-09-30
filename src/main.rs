#[macro_use]
extern crate common_failures;
extern crate link_patcher;
extern crate rprompt;
#[macro_use]
extern crate structopt;
extern crate yansi;

// -------------------------------------------------------------------------------------------------

use common_failures::prelude::*;
use std::path::PathBuf;
use structopt::StructOpt;
// -------------------------------------------------------------------------------------------------

#[derive(Debug, StructOpt)]
struct Options {
    #[structopt(parse(from_os_str))]
    input_file: PathBuf,
    #[structopt(
        short = "a",
        long = "apply_patch",
        help = "Applies the patch to the executable after a manual confirmation. A back-up of the original file is created."
    )]
    apply_patch: bool
}

// -------------------------------------------------------------------------------------------------

quick_main!(run);

fn run() -> Result<()> {
    let options = Options::from_args();

    if !yansi::Paint::enable_windows_ascii() {
        yansi::Paint::disable();
    }

    println!(concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION")));
    println!(env!("CARGO_PKG_AUTHORS"));
    println!();

    link_patcher::run(options.input_file, options.apply_patch, || {
        let prompt = yansi::Paint::red("Do you want to apply the patch now? (YES/NO): ");
        loop {
            print!("{}", prompt);
            let reply = rprompt::prompt_reply_stdout("").context("Error reading use input.")?;
            if reply.eq_ignore_ascii_case("yes") {
                println!();
                return Ok(true);
            } else if reply.eq_ignore_ascii_case("no") {
                return Ok(false);
            }
        }
    })?;
    Ok(())
}
