use std::fs::{File, OpenOptions};
use std::io;
use std::io::{BufWriter, Write};
use chrono::Utc;
use crate::result::{Error, Result};

const LOG_FILE_PATH: &str = "error_log.txt";

pub struct ErrorLogger(BufWriter<File>);

fn generate_log_line(error: &Error) -> String {
    let utc = Utc::now();
    format!("[UTC {:?}] {:?}\n", utc, error)
}

impl ErrorLogger {
    pub fn new() -> Self {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(LOG_FILE_PATH)
            .expect("failed to open or create error log file");

        let mut bw = BufWriter::new(file);
        let line = format!("\n[UTC {:?}] RESTART\n", Utc::now());
        bw.write(line.as_bytes()).expect("failed to write to error log");

        println!("opened error file");
        Self(bw)
    }

    pub fn maybe_log<T>(&mut self, result: &Result<T>) {
        if let Err(e) = result {
            let line = generate_log_line(e);
            self.0
                .write(line.as_bytes())
                .expect("failed to write to error log");
        }
    }

    pub fn flush(&mut self) -> io::Result<()> {
        println!("flushing error file");
        self.0.flush()
    }
}
