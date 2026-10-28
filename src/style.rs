//! Terminal colours: on when stdout is a terminal (or CLICOLOR_FORCE is set),
//! off when NO_COLOR is set.

use std::fmt::Display;
use std::io::IsTerminal;
use std::sync::OnceLock;

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let set = |name| std::env::var_os(name).is_some_and(|v| !v.is_empty());
        !set("NO_COLOR") && (set("CLICOLOR_FORCE") || std::io::stdout().is_terminal())
    })
}

fn paint(code: &str, text: impl Display) -> String {
    if enabled() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn bold(text: impl Display) -> String {
    paint("1", text)
}

pub fn dim(text: impl Display) -> String {
    paint("2", text)
}

pub fn red(text: impl Display) -> String {
    paint("31", text)
}

pub fn green(text: impl Display) -> String {
    paint("32", text)
}

pub fn yellow(text: impl Display) -> String {
    paint("33", text)
}

pub fn cyan(text: impl Display) -> String {
    paint("36", text)
}
