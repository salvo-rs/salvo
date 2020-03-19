use slog::{Drain, Logger};

lazy_static! {
    static ref LOGGER: Logger = {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        Logger::root(drain, o!())
    };
}
pub fn logger() -> &'static Logger {
    &LOGGER
}
