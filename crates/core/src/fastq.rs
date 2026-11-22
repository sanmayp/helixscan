use anyhow::{Result, bail};
use std::io::Read;
#[derive(Clone, Copy, Debug, Default)]
pub struct Record {
    pub seq_off: usize,
    pub seq_len: usize,
    pub qual_off: usize,
}
#[derive(Clone, Debug, Default)]
pub struct Batch {
    pub id: u64,
    pub bytes: u64,
    pub buf: Vec<u8>,
    pub records: Vec<Record>,
}
pub fn parse_records(data: &[u8], id: u64, bytes: u64) -> Result<Batch> {
    let mut batch = Batch {
        id,
        bytes,
        records: Vec::new(),
        buf: data.to_vec(),
    };
    batch.records = record_offsets(data)?;
    Ok(batch)
}
fn record_offsets(data: &[u8]) -> Result<Vec<Record>> {
    let mut records = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let mut line = [0usize; 4];
        let mut len = [0usize; 4];
        for i in 0..4 {
            let Some(rel) = data[pos..].iter().position(|b| *b == b'\n') else {
                bail!("truncated FASTQ record")
            };
            line[i] = pos;
            len[i] = rel;
            pos += rel + 1;
        }
        if data[line[0]] != b'@' || data[line[2]] != b'+' || len[1] != len[3] {
            bail!(
                "invalid FASTQ four-line structure at record {}",
                records.len()
            );
        }
        records.push(Record {
            seq_off: line[1],
            seq_len: len[1],
            qual_off: line[3],
        });
    }
    Ok(records)
}
pub struct FastqReader<R: Read> {
    reader: R,
    chunk: usize,
    tail: Vec<u8>,
    eof: bool,
    id: u64,
    offset: u64,
    recycled: Option<crossbeam_channel::Receiver<Vec<u8>>>,
}
impl<R: Read> FastqReader<R> {
    pub fn new(reader: R, chunk: usize) -> Self {
        Self {
            reader,
            chunk: chunk.max(1),
            tail: Vec::new(),
            eof: false,
            id: 0,
            offset: 0,
            recycled: None,
        }
    }
    pub fn with_recycler(
        reader: R,
        chunk: usize,
        recycled: crossbeam_channel::Receiver<Vec<u8>>,
    ) -> Self {
        let mut this = Self::new(reader, chunk);
        this.recycled = Some(recycled);
        this
    }
    pub fn next_batch(&mut self) -> Result<Option<Batch>> {
        if self.eof && self.tail.is_empty() {
            return Ok(None);
        }
        let mut data = self
            .recycled
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
            .unwrap_or_default();
        data.clear();
        data.extend_from_slice(&self.tail);
        self.tail.clear();
        if !self.eof {
            let prefix = data.len();
            let mut used = 0;
            while used < self.chunk {
                data.resize(prefix + self.chunk, 0);
                let n = self
                    .reader
                    .read(&mut data[prefix + used..prefix + self.chunk])?;
                if n == 0 {
                    self.eof = true;
                    break;
                }
                used += n;
            }
            data.truncate(prefix + used);
        }
        if self.eof {
            if data.is_empty() {
                return Ok(None);
            }
            let bytes = data.len() as u64;
            let batch = parse_records(&data, self.id, bytes)?;
            self.tail.clear();
            self.offset += bytes;
            self.id += 1;
            return Ok(Some(batch));
        }
        let mut complete = 0;
        let mut pos = 0;
        while let Some(end) = find_record(&data, pos) {
            complete = end;
            pos = end;
        }
        if complete == 0 {
            if data.len() > self.chunk.max(65536) * 4 {
                bail!("FASTQ record exceeds configured block");
            }
            self.tail = data;
            if self.tail.len() > 128 * 1024 * 1024 {
                bail!("FASTQ record exceeds 128 MiB safety limit");
            }
            return self.next_batch();
        }
        self.tail.extend_from_slice(&data[complete..]);
        let bytes = complete as u64;
        let mut batch = Batch {
            id: self.id,
            bytes,
            buf: Vec::new(),
            records: Vec::new(),
        };
        data.truncate(complete);
        batch.buf = data;
        batch.records = record_offsets(&batch.buf)?;
        self.offset += bytes;
        self.id += 1;
        Ok(Some(batch))
    }
}
fn find_record(data: &[u8], mut pos: usize) -> Option<usize> {
    let mut seq_len = 0;
    for line in 0..4 {
        let rel = data.get(pos..)?.iter().position(|b| *b == b'\n')?;
        if line == 0 && data.get(pos) != Some(&b'@') {
            return None;
        }
        if line == 1 {
            seq_len = rel;
        }
        if line == 2 && data.get(pos) != Some(&b'+') {
            return None;
        }
        if line == 3 && rel != seq_len {
            // An '@' quality line at the end of a chunk is a tail, not a bad
            // record header. Complete records are validated next.
            return None;
        }
        pos += rel + 1;
    }
    Some(pos)
}
pub fn gzip_reader(path: &std::path::Path) -> Result<Box<dyn Read + Send>> {
    let file = std::fs::File::open(path)?;
    Ok(Box::new(flate2::read::MultiGzDecoder::new(file)))
}
