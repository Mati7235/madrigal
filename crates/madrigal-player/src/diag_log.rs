use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::engine::EngineEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }

    fn parse(tag: &str) -> Option<Self> {
        match tag {
            "INFO" => Some(Level::Info),
            "WARN" => Some(Level::Warn),
            "ERROR" => Some(Level::Error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub epoch_secs: u64,
    pub level: Level,
    pub prefix: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct DiagLogConfig {
    pub max_files: usize,
    pub max_bytes: u64,
}

impl Default for DiagLogConfig {
    fn default() -> Self {
        Self {
            max_files: 5,
            max_bytes: 256 * 1024,
        }
    }
}

pub struct DiagLog {
    dir: PathBuf,
    config: DiagLogConfig,
}

const ACTIVE_NAME: &str = "diag.log";

fn archive_name(index: usize) -> String {
    format!("diag.{index}.log")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl DiagLog {
    pub fn open() -> io::Result<Self> {
        Self::open_in(
            &madrigal_paths::data_dir().join("diag-logs"),
            DiagLogConfig::default(),
        )
    }

    pub fn open_in(dir: &Path, config: DiagLogConfig) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            config: DiagLogConfig {
                max_files: config.max_files.max(2),
                max_bytes: config.max_bytes.max(64),
            },
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn push(&self, level: Level, prefix: &str, text: &str) -> io::Result<()> {
        let line = format!(
            "{} {} {}\t{}\n",
            now_secs(),
            level.tag(),
            scrub_prefix(prefix),
            scrub_text(text)
        );
        let active = self.dir.join(ACTIVE_NAME);
        let current = fs::metadata(&active).map(|m| m.len()).unwrap_or(0);
        if current > 0 && current + line.len() as u64 > self.config.max_bytes {
            self.rotate()?;
        }
        let mut data = fs::read(&active).unwrap_or_default();
        data.extend_from_slice(line.as_bytes());
        madrigal_files::atomic_write_bytes(&active, &data).map(|_| ())
    }

    pub fn push_engine_event(&self, event: &EngineEvent) -> io::Result<bool> {
        match event {
            EngineEvent::WarnLog { prefix, text } => {
                self.push(Level::Warn, prefix, text)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn read(&self, min: Level) -> io::Result<Vec<LogLine>> {
        let mut out = Vec::new();
        for path in self.files_oldest_first() {
            let text = fs::read_to_string(&path)?;
            for line in text.lines() {
                if let Some(record) = parse_line(line) {
                    if record.level >= min {
                        out.push(record);
                    }
                }
            }
        }
        Ok(out)
    }

    pub fn export(&self, dest: &Path) -> io::Result<PathBuf> {
        let mut data = Vec::new();
        for path in self.files_oldest_first() {
            data.extend_from_slice(&fs::read(&path)?);
        }
        madrigal_files::atomic_write_bytes(dest, &data)
    }

    fn files_oldest_first(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for index in (1..self.config.max_files).rev() {
            let path = self.dir.join(archive_name(index));
            if path.is_file() {
                out.push(path);
            }
        }
        let active = self.dir.join(ACTIVE_NAME);
        if active.is_file() {
            out.push(active);
        }
        out
    }

    fn rotate(&self) -> io::Result<()> {
        let oldest = self.dir.join(archive_name(self.config.max_files - 1));
        if oldest.is_file() {
            fs::remove_file(&oldest)?;
        }
        for index in (1..self.config.max_files - 1).rev() {
            let from = self.dir.join(archive_name(index));
            if from.is_file() {
                fs::rename(&from, self.dir.join(archive_name(index + 1)))?;
            }
        }
        let active = self.dir.join(ACTIVE_NAME);
        if active.is_file() {
            fs::rename(&active, self.dir.join(archive_name(1)))?;
        }
        Ok(())
    }
}

fn scrub_prefix(prefix: &str) -> String {
    prefix
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .collect()
}

fn scrub_text(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

fn parse_line(line: &str) -> Option<LogLine> {
    let (head, text) = line.split_once('\t')?;
    let mut parts = head.splitn(3, ' ');
    let epoch_secs: u64 = parts.next()?.parse().ok()?;
    let level = Level::parse(parts.next()?)?;
    let prefix = parts.next()?.to_owned();
    Some(LogLine {
        epoch_secs,
        level,
        prefix,
        text: text.to_owned(),
    })
}
