use log::{Level, Log, Metadata, Record};

pub enum RenderTarget {
    Stdout,
}

pub struct TUIProgressBar {}

pub struct TUI {
    render_info: Vec<TUIProgressBar>,
}

impl TUI {
    fn create_progress_bar(&self) -> TUIProgressBar {
        unimplemented!()
    }
    fn render(&mut self) {}
}

impl Log for TUI {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // TODO: this
        false
    }
    fn log(&self, record: &Record) {
        if self.enabled(&record.metadata()) {
            // TODO: this
        }
    }
    fn flush(&self) {}
}
