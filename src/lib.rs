use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use clap::Subcommand;

use failure::{format_err, Error};
use serde::{Deserialize, Serialize};
use serde_json::Deserializer;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Subcommand, Serialize, Deserialize)]
pub enum Commands {
    Set { key: String, value: String },
    Get { key: String },
    Rm { key: String },
}

pub struct KvStore {
    dir: PathBuf,
    index: HashMap<String, CommandPos>,
    reader: BufReaderWithPos<File>,
    writer: BufWriterWithPos<File>,
    stale_size: u64,
}

const THRESHOLD: u64 = 100;
const COMPACT_FILE_NAME: &str = "kvs.compact.log";

impl KvStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<KvStore> {
        let path = path.into();

        let mut reader = BufReaderWithPos::new(get_log_file(&path)?)?;
        let mut writer = BufWriterWithPos::new(get_log_file(&path)?)?;
        let mut stale_size = 0;
        let mut index = HashMap::new();
        let mut pos = reader.seek(SeekFrom::Start(0))?;
        writer.seek(SeekFrom::End(0))?;
        // load the data from file
        let mut stream = Deserializer::from_reader(&mut reader).into_iter::<Commands>();
        while let Some(cmd) = stream.next() {
            let new_pos = stream.byte_offset() as u64;
            match cmd? {
                Commands::Set { key, .. } => {
                    if let Some(old_cmd) = index.insert(
                        key,
                        CommandPos {
                            pos,
                            len: new_pos - pos,
                        },
                    ) {
                        stale_size += old_cmd.len;
                    }
                }
                Commands::Rm { key } => {
                    if let Some(old_cmd) = index.remove(&key) {
                        stale_size += old_cmd.len;
                    }
                }
                _ => {}
            }
            pos = new_pos;
        }

        Ok(KvStore {
            dir: path,
            index,
            reader,
            writer,
            stale_size,
        })
    }

    pub fn set(&mut self, key: String, value: String) -> Result<()> {
        let cmd = Commands::Set {
            key: key.clone(),
            value,
        };
        let pos = self.writer.pos;
        serde_json::to_writer(&mut self.writer, &cmd)?;
        self.writer.flush()?;
        let len = self.writer.pos - pos;
        if let Some(old_cmd) = self.index.insert(key, CommandPos { pos, len }) {
            self.stale_size += old_cmd.len;
        }

        if self.stale_size > THRESHOLD {
            self.compact()?;
        }

        Ok(())
    }

    pub fn get(&mut self, key: String) -> Result<Option<String>> {
        if let Some(cmd) = self.index.get(&key) {
            self.reader.seek(SeekFrom::Start(cmd.pos))?;
            let cmd_reader = self.reader.by_ref().take(cmd.len);
            if let Commands::Set { value, .. } = serde_json::from_reader(cmd_reader)? {
                Ok(Some(value))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub fn remove(&mut self, key: String) -> Result<()> {
        if self.index.get(&key).is_none() {
            return Err(format_err!("Key not found"));
        }
        let cmd = Commands::Rm { key: key.clone() };
        serde_json::to_writer(&mut self.writer, &cmd)?;
        self.writer.flush()?;
        if let Some(old_cmd) = self.index.remove(&key) {
            self.stale_size += old_cmd.len;
        }
        if self.stale_size > THRESHOLD {
            self.compact()?;
        }

        Ok(())
    }

    fn compact(&mut self) -> Result<()> {
        let file = self.dir.join(COMPACT_FILE_NAME);
        let mut compact_writer = BufWriterWithPos::new(open_file(&file)?)?;
        for cmd_pos in &mut self.index.values_mut() {
            self.reader.seek(SeekFrom::Start(cmd_pos.pos))?;
            let mut cmd_reader = self.reader.by_ref().take(cmd_pos.len);
            let pos = compact_writer.pos;
            let len = io::copy(&mut cmd_reader, &mut compact_writer)?;
            compact_writer.flush()?;
            *cmd_pos = CommandPos { pos, len };
        }
        compact_writer.flush()?;

        // Delete old file
        std::fs::remove_file(self.dir.join("kvs.log"))?;
        // Rename compact file
        std::fs::rename(file, self.dir.join("kvs.log"))?;

        // reconfigure the reader and writer
        self.reader = BufReaderWithPos::new(get_log_file(&self.dir)?)?;
        self.writer = BufWriterWithPos::new(get_log_file(&self.dir)?)?;
        self.reader.seek(SeekFrom::Start(0))?;
        self.writer.seek(SeekFrom::End(0))?;
        self.stale_size = 0;
        Ok(())
    }
}

fn open_file(path: &Path) -> Result<File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;
    Ok(file)
}

fn get_log_file(path: &Path) -> Result<File> {
    let log_path = path.join("kvs.log");
    open_file(&log_path)
}

#[derive(Debug)]
struct CommandPos {
    pos: u64,
    len: u64,
}

struct BufReaderWithPos<R: Read + Seek> {
    reader: BufReader<R>,
    pos: u64,
}

impl<R: Read + Seek> BufReaderWithPos<R> {
    fn new(mut inner: R) -> Result<Self> {
        let pos = inner.seek(SeekFrom::Current(0))?;
        Ok(BufReaderWithPos {
            reader: BufReader::new(inner),
            pos,
        })
    }
}

impl<R: Read + Seek> Read for BufReaderWithPos<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = self.reader.read(buf)?;
        self.pos += len as u64;
        Ok(len)
    }
}

impl<R: Read + Seek> Seek for BufReaderWithPos<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.pos = self.reader.seek(pos)?;
        Ok(self.pos)
    }
}

struct BufWriterWithPos<W: Write + Seek> {
    writer: BufWriter<W>,
    pos: u64,
}

impl<W: Write + Seek> BufWriterWithPos<W> {
    fn new(mut inner: W) -> Result<Self> {
        let pos = inner.seek(SeekFrom::Current(0))?;
        Ok(BufWriterWithPos {
            writer: BufWriter::new(inner),
            pos,
        })
    }
}

impl<W: Write + Seek> Write for BufWriterWithPos<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = self.writer.write(buf)?;
        self.pos += len as u64;
        Ok(len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

impl<W: Write + Seek> Seek for BufWriterWithPos<W> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.pos = self.writer.seek(pos)?;
        Ok(self.pos)
    }
}
