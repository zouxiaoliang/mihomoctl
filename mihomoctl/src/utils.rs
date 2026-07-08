use clap_complete::Shell;
use env_logger::fmt::Color;
use env_logger::Builder;
use log::{Level, LevelFilter};
use std::{env, io, path::PathBuf};

pub fn detect_shell() -> Option<Shell> {
    match env::var("SHELL") {
        Ok(shell) => PathBuf::from(shell)
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse().ok()),
        Err(_) => None,
    }
}

pub fn terminal_width(default: u16) -> u16 {
    terminal_width_from_size(crossterm::terminal::size(), default)
}

fn terminal_width_from_size(size: io::Result<(u16, u16)>, default: u16) -> u16 {
    size.map(|(width, _)| width).unwrap_or(default)
}

pub fn init_logger(level: Option<LevelFilter>) {
    let mut builder = Builder::new();

    if let Some(lf) = level {
        builder.filter_level(lf);
    } else if let Ok(s) = ::std::env::var("MIHOMOCTL_LOG") {
        builder.parse_filters(&s);
    } else {
        builder.filter_level(LevelFilter::Info);
    }

    builder.format(|f, record| {
        use std::io::Write;
        let mut style = f.style();

        let level = match record.level() {
            Level::Trace => style.set_color(Color::Magenta).value("Trace"),
            Level::Debug => style.set_color(Color::Blue).value("Debug"),
            Level::Info => style.set_color(Color::Green).value(" Info"),
            Level::Warn => style.set_color(Color::Yellow).value(" Warn"),
            Level::Error => style.set_color(Color::Red).value("Error"),
        };

        writeln!(f, " {} > {}", level, record.args(),)
    });

    builder.init()
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::terminal_width_from_size;

    #[test]
    fn terminal_width_from_size_uses_reported_width() {
        assert_eq!(terminal_width_from_size(Ok((120, 40)), 70), 120);
    }

    #[test]
    fn terminal_width_from_size_uses_default_on_error() {
        let err = io::Error::new(io::ErrorKind::Other, "terminal size unavailable");

        assert_eq!(terminal_width_from_size(Err(err), 70), 70);
    }
}
