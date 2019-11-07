use std::sync::Mutex;
use slog::{Logger, Drain};

lazy_static! {
    static ref LOGGER: Logger = {
         let decorator = slog_term::TermDecorator::new().build();
        let drain = Mutex::new(slog_term::FullFormat::new(decorator).build()).fuse();
        Logger::root(drain, o!("version" => env!("CARGO_PKG_VERSION")))
    };
}
pub fn logger() -> &'static Logger {
    &LOGGER
}