mod_use::mod_use![command, proxy_render, utils, error, interactive, ui];

pub use clap;
use log::debug;

use crate::clap::Parser as _;

pub fn run() {
    let opts = Opts::parse();
    opts.init_logger();
    debug!("Opts: {:#?}", opts);

    if let Err(e) = match opts.cmd {
        None => ui::main_loop(Default::default(), opts.flag).map_err(Into::into),
        Some(Cmd::Tui(opt)) => ui::main_loop(opt, opts.flag).map_err(Into::into),
        Some(Cmd::Proxy(sub)) => sub.handle(&opts.flag),
        Some(Cmd::Server(sub)) => sub.handle(&opts.flag),
        Some(Cmd::Completion(arg)) => arg.handle(),
    } {
        eprintln!("{}", e)
    }
}

fn main() {
    run()
}
