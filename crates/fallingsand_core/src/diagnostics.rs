use std::any::Any;
use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;

const UNKNOWN: &str = "unknown panic payload";

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(report_panic));
}

pub fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => payload
            .downcast::<&str>()
            .map_or_else(|_| UNKNOWN.to_string(), |message| message.to_string()),
    }
}

fn report_panic(info: &PanicHookInfo) {
    let backtrace = Backtrace::force_capture();
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("unnamed");
    let location = info.location().map_or(String::new(), |at| at.to_string());
    let message = info.payload_as_str().unwrap_or(UNKNOWN);
    eprintln!("thread '{name}' panicked at {location}:\n{message}\n{backtrace}");
}
