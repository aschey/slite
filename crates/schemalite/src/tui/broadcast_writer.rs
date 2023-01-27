use once_cell::sync::OnceCell;
use tokio::sync::broadcast;
use tracing_subscriber::fmt::MakeWriter;

static LOG_SENDER: OnceCell<broadcast::Sender<String>> = OnceCell::new();

pub struct BroadcastWriter {
    log_sender: broadcast::Sender<String>,
}

impl BroadcastWriter {
    pub fn receiver(&self) -> broadcast::Receiver<String> {
        self.log_sender.subscribe()
    }
}

impl Default for BroadcastWriter {
    fn default() -> Self {
        let log_sender = LOG_SENDER
            .get_or_init(|| {
                let (tx, _) = broadcast::channel(32);
                tx
            })
            .clone();
        Self { log_sender }
    }
}

impl std::io::Write for BroadcastWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let buf_len = buf.len();

        self.log_sender
            .send(std::str::from_utf8(buf).unwrap().to_owned())
            .ok();
        Ok(buf_len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BroadcastWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        Self::default()
    }
}
