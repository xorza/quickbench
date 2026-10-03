use std::io::{self, IsTerminal};

pub(crate) const RESET: &str = "\x1b[0m";
pub(crate) const BOLD: &str = "\x1b[1m";
pub(crate) const CYAN: &str = "\x1b[36m";
pub(crate) const YELLOW: &str = "\x1b[33m";
pub(crate) const GREEN: &str = "\x1b[32m";
pub(crate) const RED: &str = "\x1b[31m";
pub(crate) const DIM: &str = "\x1b[2m";

/// Whether the report goes to a terminal that renders the escape codes above.
pub(crate) fn enabled() -> bool {
    io::stdout().is_terminal()
}
