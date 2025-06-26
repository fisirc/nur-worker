use core::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use env_logger::fmt::{Color, Style, StyledValue};
use log::Level;

static MAX_MODULE_WIDTH: AtomicUsize = AtomicUsize::new(0);

pub fn build_logger() -> env_logger::Builder {
    let mut builder = env_logger::Builder::new();

    let pkg_name = crate::env::CARGO_PKG_NAME.clone();
    let pkg_name: &'static str = pkg_name.leak();
    let pkg_name_len = pkg_name.len();

    builder.format(move |f, record| {
        use std::io::Write;
        let mut target = record.target();
        if target.starts_with(pkg_name) {
            if target.len() == pkg_name_len {
                target = "nur";
            } else {
                target = &target[pkg_name.len() + 2..];
            }
        }

        let max_width = max_target_width(target);

        let mut style = f.style();
        let level = colored_level(&mut style, record.level());

        let mut style = f.style();
        let target = style.set_bold(true).value(Padded {
            value: target,
            width: max_width,
        });

        let time = f.timestamp_micros();
        writeln!(f, "{time} {level} {target} > {}", record.args(),)
    });

    if std::env::var_os("RUST_LOG").is_none() {
        builder.filter_level(log::LevelFilter::Debug);
    }

    builder.parse_env("RUST_LOG");

    builder
}

struct Padded<T> {
    value: T,
    width: usize,
}

impl<T: fmt::Display> fmt::Display for Padded<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{: <width$}", self.value, width = self.width)
    }
}

fn max_target_width(target: &str) -> usize {
    let max_width = MAX_MODULE_WIDTH.load(Ordering::Relaxed);
    if max_width < target.len() {
        MAX_MODULE_WIDTH.store(target.len(), Ordering::Relaxed);
        target.len()
    } else {
        max_width
    }
}

fn colored_level<'a>(style: &'a mut Style, level: Level) -> StyledValue<'a, &'static str> {
    match level {
        Level::Trace => style.set_color(Color::Magenta).value("TRACE"),
        Level::Debug => style.set_color(Color::Blue).value("DEBUG"),
        Level::Info => style.set_color(Color::Green).value("INFO "),
        Level::Warn => style.set_color(Color::Yellow).value("WARN "),
        Level::Error => style.set_color(Color::Red).value("ERROR"),
    }
}
