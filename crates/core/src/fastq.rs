use anyhow::{Result, bail};
use std::io::{BufReader, Read};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
            let Some(rel) = memchr::memchr(b'\n', &data[pos..]) else {
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
        let prefix = self.tail.len();
        data.extend_from_slice(&self.tail);
        self.tail.clear();

        if !self.eof {
            data.reserve(self.chunk);
            while data.len() - prefix < self.chunk {
                // Read into spare, uninitialized capacity, so no per-read
                // resize can clear several MiB that will immediately be filled.
                let spare = unsafe {
                    std::slice::from_raw_parts_mut(
                        data.as_mut_ptr().add(data.len()),
                        data.capacity() - data.len(),
                    )
                };
                let n = self.reader.read(spare)?;
                // SAFETY: `read` initializes exactly n bytes of `spare`, and
                // the buffer was reserved before any pointer was derived.
                unsafe { data.set_len(data.len() + n) };
                if n == 0 {
                    self.eof = true;
                    break;
                }
            }
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
        // Discover boundaries and record offsets together. Parsing the same
        // batch a second time needlessly limits the sole input thread.
        let (records, complete) = record_prefix(&data)?;
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
            buf: data,
            records: Vec::new(),
        };
        batch.buf.truncate(complete);
        batch.records = records;
        self.offset += bytes;
        self.id += 1;
        Ok(Some(batch))
    }
}
fn record_prefix(data: &[u8]) -> Result<(Vec<Record>, usize)> {
    let mut records = Vec::new();
    let mut pos = 0;
    let mut complete = 0;
    while pos < data.len() {
        let mut line = [0usize; 4];
        let mut len = [0usize; 4];
        let mut truncated = false;
        for i in 0..4 {
            let Some(rel) = memchr::memchr(b'\n', &data[pos..]) else {
                truncated = true;
                break;
            };
            line[i] = pos;
            len[i] = rel;
            pos += rel + 1;
        }
        if truncated {
            break;
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
        complete = pos;
    }
    Ok((records, complete))
}
pub fn gzip_reader(file: std::fs::File) -> Result<Box<dyn Read + Send>> {
    let buffered = BufReader::with_capacity(1024 * 1024, file);
    Ok(Box::new(flate2::bufread::MultiGzDecoder::new(buffered)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_matches_uncompressed_offsets_across_boundaries() {
        let fastq = b"@r\nACGT\n+\n@!!!\n@r2\nACGT\n+\n!!!!\n";
        let path = std::env::temp_dir().join(format!(
            "helixscan-fastq-gzip-{:?}.gz",
            std::thread::current().id()
        ));
        let mut encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(&path).unwrap(),
            flate2::Compression::fast(),
        );
        std::io::Write::write_all(&mut encoder, fastq).unwrap();
        encoder.finish().unwrap();
        for chunk in 1..fastq.len() {
            let mut plain = FastqReader::new(std::io::Cursor::new(fastq), chunk);
            let mut compressed = FastqReader::new(
                gzip_reader(std::fs::File::open(&path).unwrap()).unwrap(),
                chunk,
            );
            loop {
                let expected = plain.next_batch().unwrap();
                let actual = compressed.next_batch().unwrap();
                assert_eq!(expected.is_some(), actual.is_some());
                let (Some(expected), Some(actual)) = (expected, actual) else {
                    break;
                };
                assert_eq!(expected.buf, actual.buf);
                assert_eq!(expected.records.len(), actual.records.len());
                for (expected, actual) in expected.records.iter().zip(&actual.records) {
                    assert_eq!(expected, actual);
                }
            }
        }
        std::fs::remove_file(path).unwrap();
    }
}
