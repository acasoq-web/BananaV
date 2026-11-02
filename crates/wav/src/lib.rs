//! Sound files from their specifications: WAV (RIFF/WAVE, and RF64 for
//! files over 4 GB; Microsoft/IBM and EBU Tech 3306), Sony Wave64, AIFF.
//!
//! Reading (see [`read_info`]) has random access (only the frames asked for
//! are read) and also takes AIFF-C, AU and CAF, µ-law/A-law and ADPCM.
//! Writing streams PCM of 16/24 bits or 32-bit floats.

use std::io::{self, Read, Seek, SeekFrom, Write};

mod aiff;
mod read;
mod w64;

pub use aiff::AiffWriter;
pub use read::{Container, WavInfo, WavReader, read_info};
pub use w64::W64Writer;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    NotWav,
    Unsupported(String),
    Corrupt(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "erro de leitura: {e}"),
            Error::NotWav => f.write_str("não é um arquivo WAV"),
            Error::Unsupported(s) => write!(f, "WAV não suportado: {s}"),
            Error::Corrupt(s) => write!(f, "WAV corrompido: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleFormat {
    /// PCM inteiro: 8 bits sem sinal, 16/24/32 com sinal.
    Int(u16),
    /// Float IEEE de 32 ou 64 bits.
    Float(u16),
}

impl SampleFormat {
    pub fn bytes(self) -> usize {
        match self {
            SampleFormat::Int(b) | SampleFormat::Float(b) => b as usize / 8,
        }
    }
}

const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().expect("8 bytes"))
}

/// Broadcast Wave time reference: the sample (since midnight) where the
/// recording starts, from the `bext` chunk. `None` for plain WAV files.
pub fn bext_time_reference<R: Read + Seek>(r: &mut R) -> Result<Option<u64>> {
    Ok(read_bext(r)?.map(|b| b.time_reference))
}

/// A Broadcast Wave file's `bext` chunk (its texts up to their first NUL).
/// `None` for plain WAV files.
pub fn read_bext<R: Read + Seek>(r: &mut R) -> Result<Option<Bext>> {
    r.seek(SeekFrom::Start(0))?;
    let mut head = [0u8; 12];
    r.read_exact(&mut head).map_err(|_| Error::NotWav)?;
    if !matches!(&head[..4], b"RIFF" | b"RF64" | b"BW64") || &head[8..12] != b"WAVE" {
        return Err(Error::NotWav);
    }
    let mut pos = 12u64;
    loop {
        let mut ch = [0u8; 8];
        if r.read_exact(&mut ch).is_err() {
            return Ok(None);
        }
        let size = u32le(&ch[4..]) as u64;
        if &ch[..4] == b"bext" {
            // Description (256) + originator (32) + reference (32) + date (10)
            // + time (8) come before the 64-bit time reference.
            if size < 346 {
                return Ok(None);
            }
            let mut b = [0u8; 346];
            r.read_exact(&mut b)?;
            let text = |at: usize, len: usize| {
                let field = &b[at..at + len];
                let end = field.iter().position(|&c| c == 0).unwrap_or(len);
                String::from_utf8_lossy(&field[..end])
                    .trim_end()
                    .to_string()
            };
            return Ok(Some(Bext {
                description: text(0, 256),
                originator: text(256, 32),
                originator_reference: text(288, 32),
                date: text(320, 10),
                time: text(330, 8),
                time_reference: u64le(&b[338..]),
            }));
        }
        if &ch[..4] == b"data" && size == 0xFFFF_FFFF {
            return Ok(None);
        }
        pos += 8 + size + (size & 1);
        r.seek(SeekFrom::Start(pos))?;
    }
}

/// Planar frames as interleaved little-endian samples of `format` (8-bit
/// PCM is unsigned, centered on 128).
pub(crate) fn encode_le(format: SampleFormat, channels: &[Vec<f32>]) -> Vec<u8> {
    let n = channels.first().map_or(0, Vec::len);
    let mut buf = Vec::with_capacity(n * format.bytes() * channels.len());
    for i in 0..n {
        for ch in channels {
            let s = ch[i].clamp(-1.0, 1.0);
            match format {
                SampleFormat::Int(8) => buf.push(((s * 127.0).round() + 128.0) as u8),
                SampleFormat::Int(16) => {
                    buf.extend_from_slice(&((s * 32767.0).round() as i16).to_le_bytes())
                }
                SampleFormat::Int(24) => {
                    let v = (s * 8_388_607.0).round() as i32;
                    buf.extend_from_slice(&v.to_le_bytes()[..3]);
                }
                SampleFormat::Int(_) => {
                    let v = (s as f64 * 2_147_483_647.0).round() as i32;
                    buf.extend_from_slice(&v.to_le_bytes());
                }
                SampleFormat::Float(_) => buf.extend_from_slice(&ch[i].to_le_bytes()),
            }
        }
    }
    buf
}

pub(crate) fn write_zeros(out: &mut impl Write, n: u64) -> io::Result<()> {
    for _ in 0..n {
        out.write_all(&[0])?;
    }
    Ok(())
}

/// Broadcast Wave metadata (the `bext` chunk, EBU Tech 3285): who made the
/// file, when, and where it belongs on a timeline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bext {
    pub description: String,
    /// The program or person that made it.
    pub originator: String,
    /// A reference unique to this file.
    pub originator_reference: String,
    /// `yyyy-mm-dd` and `hh:mm:ss`.
    pub date: String,
    pub time: String,
    /// The sample (of each channel) where the recording starts.
    pub time_reference: u64,
}

impl Bext {
    /// The chunk's body: version 1, no UMID, no coding history.
    fn body(&self) -> Vec<u8> {
        let mut b = vec![0u8; 602];
        let put = |b: &mut Vec<u8>, at: usize, len: usize, text: &str| {
            let bytes = text.as_bytes();
            let n = bytes.len().min(len);
            b[at..at + n].copy_from_slice(&bytes[..n]);
        };
        put(&mut b, 0, 256, &self.description);
        put(&mut b, 256, 32, &self.originator);
        put(&mut b, 288, 32, &self.originator_reference);
        put(&mut b, 320, 10, &self.date);
        put(&mut b, 330, 8, &self.time);
        b[338..346].copy_from_slice(&self.time_reference.to_le_bytes());
        // Version 1.
        b[346..348].copy_from_slice(&1u16.to_le_bytes());
        b
    }
}

/// A marker or region of a WAV file: a `cue` point with a `labl` name and,
/// for a region, an `ltxt` length.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cue {
    /// Frame (sample of each channel) it marks.
    pub frame: u32,
    /// A region's length in frames (0: a marker).
    pub length: u32,
    pub name: String,
}

/// The `cue ` and `LIST`/`adtl` chunks of `cues` (nothing for none).
fn cue_chunks(cues: &[Cue]) -> Vec<u8> {
    if cues.is_empty() {
        return Vec::new();
    }
    let mut cue = Vec::new();
    cue.extend_from_slice(&(cues.len() as u32).to_le_bytes());
    let mut adtl = Vec::new();
    adtl.extend_from_slice(b"adtl");
    for (i, c) in cues.iter().enumerate() {
        let id = i as u32 + 1;
        cue.extend_from_slice(&id.to_le_bytes());
        cue.extend_from_slice(&c.frame.to_le_bytes());
        cue.extend_from_slice(b"data");
        cue.extend_from_slice(&0u32.to_le_bytes());
        cue.extend_from_slice(&0u32.to_le_bytes());
        cue.extend_from_slice(&c.frame.to_le_bytes());
        let mut text = c.name.as_bytes().to_vec();
        text.push(0);
        adtl.extend_from_slice(b"labl");
        adtl.extend_from_slice(&(4 + text.len() as u32).to_le_bytes());
        adtl.extend_from_slice(&id.to_le_bytes());
        if text.len() % 2 == 1 {
            text.push(0);
        }
        adtl.extend_from_slice(&text);
        if c.length > 0 {
            // A region: its length, purpose "rgn ", no country or language.
            adtl.extend_from_slice(b"ltxt");
            adtl.extend_from_slice(&20u32.to_le_bytes());
            adtl.extend_from_slice(&id.to_le_bytes());
            adtl.extend_from_slice(&c.length.to_le_bytes());
            adtl.extend_from_slice(b"rgn ");
            adtl.extend_from_slice(&[0u8; 8]);
        }
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"cue ");
    out.extend_from_slice(&(cue.len() as u32).to_le_bytes());
    out.extend_from_slice(&cue);
    out.extend_from_slice(b"LIST");
    out.extend_from_slice(&(adtl.len() as u32).to_le_bytes());
    out.extend_from_slice(&adtl);
    out
}

/// A chunk of a RIFF file: its id, where its header starts, its size and,
/// for a `LIST`, its type.
struct Chunk {
    id: [u8; 4],
    at: u64,
    size: u64,
    list: Option<[u8; 4]>,
}

impl Chunk {
    fn end(&self) -> u64 {
        self.at + 8 + self.size + (self.size & 1)
    }

    /// A chunk of markers: `cue ` or a `LIST` of `adtl` texts.
    fn is_cues(&self) -> bool {
        &self.id == b"cue " || (&self.id == b"LIST" && self.list == Some(*b"adtl"))
    }
}

/// The chunks of a WAV file (RIFF or RF64), up to its end; whether it is
/// RF64.
fn chunks<R: Read + Seek>(r: &mut R) -> Result<(Vec<Chunk>, bool)> {
    let len = r.seek(SeekFrom::End(0))?;
    r.seek(SeekFrom::Start(0))?;
    let mut head = [0u8; 12];
    r.read_exact(&mut head).map_err(|_| Error::NotWav)?;
    let rf64 = matches!(&head[..4], b"RF64" | b"BW64");
    if !(rf64 || &head[..4] == b"RIFF") || &head[8..12] != b"WAVE" {
        return Err(Error::NotWav);
    }
    let mut data64 = None;
    let mut out = Vec::new();
    let mut at = 12u64;
    while at + 8 <= len {
        r.seek(SeekFrom::Start(at))?;
        let mut ch = [0u8; 12];
        r.read_exact(&mut ch[..8])?;
        let id: [u8; 4] = ch[..4].try_into().expect("4 bytes");
        let mut size = u32le(&ch[4..8]) as u64;
        if &id == b"ds64" && size >= 16 {
            let mut b = [0u8; 16];
            r.read_exact(&mut b)?;
            data64 = Some(u64le(&b[8..]));
            r.seek(SeekFrom::Start(at + 8))?;
        }
        if &id == b"data" && size == 0xFFFF_FFFF {
            size = data64.unwrap_or(len - at - 8);
        }
        let list = if &id == b"LIST" && size >= 4 {
            r.read_exact(&mut ch[8..12])?;
            Some(ch[8..12].try_into().expect("4 bytes"))
        } else {
            None
        };
        let c = Chunk { id, at, size, list };
        at = c.end();
        out.push(c);
    }
    Ok((out, rf64))
}

/// The markers and regions of a WAV file (`cue ` points named by `labl`,
/// regions by `ltxt` lengths), in time order.
pub fn read_cues<R: Read + Seek>(r: &mut R) -> Result<Vec<Cue>> {
    let (chunks, _) = chunks(r)?;
    let mut points: Vec<(u32, Cue)> = Vec::new();
    let mut names = std::collections::HashMap::new();
    let mut lengths = std::collections::HashMap::new();
    for c in chunks.iter().filter(|c| c.is_cues()) {
        let size = c.size.min(16 << 20) as usize;
        r.seek(SeekFrom::Start(c.at + 8))?;
        let mut body = vec![0u8; size];
        r.read_exact(&mut body)?;
        if &c.id == b"cue " {
            let n = body.get(..4).map_or(0, u32le) as usize;
            for p in body[4..].as_chunks::<24>().0.iter().take(n) {
                let id = u32le(&p[..4]);
                // The sample offset, as written by editors (the position
                // is the same for plain PCM).
                let frame = u32le(&p[20..24]);
                points.push((
                    id,
                    Cue {
                        frame,
                        ..Cue::default()
                    },
                ));
            }
            continue;
        }
        let mut i = 4;
        while i + 8 <= body.len() {
            let sub = &body[i..i + 4];
            let len = u32le(&body[i + 4..i + 8]) as usize;
            let data = &body[(i + 8).min(body.len())..(i + 8 + len).min(body.len())];
            if data.len() >= 4 {
                let id = u32le(&data[..4]);
                let text = || {
                    let t = &data[4..];
                    let t = &t[..t.iter().position(|&b| b == 0).unwrap_or(t.len())];
                    String::from_utf8_lossy(t).into_owned()
                };
                match sub {
                    b"labl" => {
                        names.insert(id, text());
                    }
                    // A note names a point without a label.
                    b"note" => {
                        names.entry(id).or_insert_with(text);
                    }
                    b"ltxt" if data.len() >= 8 => {
                        lengths.insert(id, u32le(&data[4..8]));
                    }
                    _ => {}
                }
            }
            i += 8 + len + (len & 1);
        }
    }
    let mut out: Vec<Cue> = points
        .into_iter()
        .map(|(id, mut c)| {
            c.name = names.remove(&id).unwrap_or_default();
            c.length = lengths.get(&id).copied().unwrap_or(0);
            c
        })
        .collect();
    out.sort_by_key(|c| c.frame);
    Ok(out)
}

/// Writes `cues` as the file's markers and regions, in place: the audio
/// stays where it is (markers before it become `JUNK`; the new ones go at
/// the end). Plain RIFF files only (RF64 sizes are fixed by its `ds64`).
pub fn write_cues(file: &mut std::fs::File, cues: &[Cue]) -> Result<()> {
    let (chunks, rf64) = chunks(file)?;
    if rf64 {
        return Err(Error::Unsupported("RF64".into()));
    }
    if !chunks.iter().any(|c| &c.id == b"data") {
        return Err(Error::Corrupt("no data chunk".into()));
    }
    // Where the file ends once its trailing markers are dropped.
    let keep = chunks
        .iter()
        .rev()
        .find(|c| !c.is_cues())
        .map_or(12, Chunk::end);
    for c in chunks.iter().filter(|c| c.is_cues() && c.at < keep) {
        file.seek(SeekFrom::Start(c.at))?;
        file.write_all(b"JUNK")?;
    }
    let new = cue_chunks(cues);
    let end = keep + new.len() as u64;
    if end - 8 > u32::MAX as u64 {
        return Err(Error::Unsupported("over 4 GB".into()));
    }
    file.set_len(keep)?;
    file.seek(SeekFrom::Start(keep))?;
    file.write_all(&new)?;
    file.seek(SeekFrom::Start(4))?;
    file.write_all(&((end - 8) as u32).to_le_bytes())?;
    file.flush()?;
    Ok(())
}

/// Escritor em fluxo. Os tamanhos são corrigidos em [`WavWriter::finish`];
/// se os dados passarem de 4 GB, o arquivo vira RF64.
#[derive(Debug)]
pub struct WavWriter<W: Write + Seek> {
    inner: W,
    format: SampleFormat,
    channels: u16,
    data_bytes: u64,
    data_start: u64,
    cues: Vec<Cue>,
    /// The project the file was rendered from: a `bvpr` chunk.
    project: String,
}

/// The project a WAV file was rendered from, when its renderer kept the
/// path (BananaV's `bvpr` chunk: Render's "Save project as path
/// reference in rendered file").
pub fn read_project_reference<R: Read + Seek>(r: &mut R) -> Result<Option<String>> {
    let (chunks, _) = chunks(r)?;
    let Some(c) = chunks.iter().find(|c| &c.id == b"bvpr") else {
        return Ok(None);
    };
    r.seek(SeekFrom::Start(c.at + 8))?;
    let mut body = vec![0u8; c.size.min(1 << 16) as usize];
    r.read_exact(&mut body)?;
    Ok(String::from_utf8(body)
        .ok()
        .map(|s| s.trim_end_matches('\0').to_string())
        .filter(|s| !s.is_empty()))
}

/// Tamanho do chunk JUNK reservado para virar ds64 se preciso.
const JUNK_SIZE: u32 = 28;

impl<W: Write + Seek> WavWriter<W> {
    pub fn new(
        inner: W,
        sample_rate: u32,
        channels: u16,
        format: SampleFormat,
    ) -> Result<WavWriter<W>> {
        WavWriter::with_info(inner, sample_rate, channels, format, &[])
    }

    /// Like [`new`](Self::new), with a `LIST`/`INFO` chunk before the data:
    /// (four-letter id, text) such as `(*b"INAM", "Title")`. Empty texts are
    /// left out.
    pub fn with_info(
        inner: W,
        sample_rate: u32,
        channels: u16,
        format: SampleFormat,
        info: &[([u8; 4], &str)],
    ) -> Result<WavWriter<W>> {
        WavWriter::with_metadata(inner, sample_rate, channels, format, info, None)
    }

    /// Like [`with_info`](Self::with_info), with a Broadcast Wave `bext`
    /// chunk too.
    pub fn with_metadata(
        mut inner: W,
        sample_rate: u32,
        channels: u16,
        format: SampleFormat,
        info: &[([u8; 4], &str)],
        bext: Option<&Bext>,
    ) -> Result<WavWriter<W>> {
        let (tag, bits) = match format {
            SampleFormat::Int(b @ (8 | 16 | 24 | 32)) => (WAVE_FORMAT_PCM, b),
            SampleFormat::Float(32) => (WAVE_FORMAT_IEEE_FLOAT, 32),
            other => return Err(Error::Unsupported(format!("escrita em {other:?}"))),
        };
        let block = channels as u32 * bits as u32 / 8;
        let mut h = Vec::with_capacity(80);
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&0u32.to_le_bytes());
        h.extend_from_slice(b"WAVE");
        h.extend_from_slice(b"JUNK");
        h.extend_from_slice(&JUNK_SIZE.to_le_bytes());
        h.extend_from_slice(&[0u8; JUNK_SIZE as usize]);
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&tag.to_le_bytes());
        h.extend_from_slice(&channels.to_le_bytes());
        h.extend_from_slice(&sample_rate.to_le_bytes());
        h.extend_from_slice(&(sample_rate * block).to_le_bytes());
        h.extend_from_slice(&(block as u16).to_le_bytes());
        h.extend_from_slice(&bits.to_le_bytes());
        if let Some(b) = bext {
            let body = b.body();
            h.extend_from_slice(b"bext");
            h.extend_from_slice(&(body.len() as u32).to_le_bytes());
            h.extend_from_slice(&body);
        }
        let mut list = Vec::new();
        for (id, text) in info.iter().filter(|(_, t)| !t.is_empty()) {
            // Zero-terminated, padded to an even size.
            let mut data = text.as_bytes().to_vec();
            data.push(0);
            list.extend_from_slice(id);
            list.extend_from_slice(&(data.len() as u32).to_le_bytes());
            if data.len() % 2 == 1 {
                data.push(0);
            }
            list.extend_from_slice(&data);
        }
        if !list.is_empty() {
            h.extend_from_slice(b"LIST");
            h.extend_from_slice(&(list.len() as u32 + 4).to_le_bytes());
            h.extend_from_slice(b"INFO");
            h.extend_from_slice(&list);
        }
        h.extend_from_slice(b"data");
        h.extend_from_slice(&0u32.to_le_bytes());
        inner.write_all(&h)?;
        Ok(WavWriter {
            inner,
            format,
            channels,
            data_bytes: 0,
            data_start: h.len() as u64,
            cues: Vec::new(),
            project: String::new(),
        })
    }

    /// Markers written after the audio (`cue ` and `LIST`/`adtl` chunks).
    /// The project the file was rendered from, kept in a `bvpr` chunk.
    pub fn set_project(&mut self, path: &str) {
        self.project = path.to_string();
    }

    pub fn set_cues(&mut self, cues: Vec<Cue>) {
        self.cues = cues;
    }

    /// Bytes of the file so far.
    pub fn bytes_written(&self) -> u64 {
        self.data_start + self.data_bytes
    }

    /// Escreve quadros planares (um vetor por canal, todos do mesmo tamanho).
    pub fn write_planar(&mut self, channels: &[Vec<f32>]) -> Result<()> {
        assert_eq!(channels.len(), self.channels as usize, "número de canais");
        let buf = encode_le(self.format, channels);
        self.inner.write_all(&buf)?;
        self.data_bytes += buf.len() as u64;
        Ok(())
    }

    /// Fecha os tamanhos dos chunks e devolve o destino.
    pub fn finish(mut self) -> Result<W> {
        if self.data_bytes & 1 == 1 {
            self.inner.write_all(&[0])?;
        }
        let mut cues = cue_chunks(&self.cues);
        if !self.project.is_empty() {
            let text = self.project.as_bytes();
            cues.extend_from_slice(b"bvpr");
            cues.extend_from_slice(&(text.len() as u32).to_le_bytes());
            cues.extend_from_slice(text);
            if text.len() & 1 == 1 {
                cues.push(0);
            }
        }
        self.inner.write_all(&cues)?;
        let riff =
            self.data_start - 8 + self.data_bytes + (self.data_bytes & 1) + cues.len() as u64;
        if riff <= u32::MAX as u64 {
            self.inner.seek(SeekFrom::Start(4))?;
            self.inner.write_all(&(riff as u32).to_le_bytes())?;
            self.inner.seek(SeekFrom::Start(self.data_start - 4))?;
            self.inner
                .write_all(&(self.data_bytes as u32).to_le_bytes())?;
        } else {
            // RF64: o JUNK vira ds64 com os tamanhos de 64 bits.
            let frames = self.data_bytes / (self.format.bytes() as u64 * self.channels as u64);
            self.inner.seek(SeekFrom::Start(0))?;
            self.inner.write_all(b"RF64")?;
            self.inner.write_all(&u32::MAX.to_le_bytes())?;
            self.inner.seek(SeekFrom::Start(12))?;
            self.inner.write_all(b"ds64")?;
            self.inner.write_all(&JUNK_SIZE.to_le_bytes())?;
            self.inner.write_all(&riff.to_le_bytes())?;
            self.inner.write_all(&self.data_bytes.to_le_bytes())?;
            self.inner.write_all(&frames.to_le_bytes())?;
            self.inner.write_all(&0u32.to_le_bytes())?;
            self.inner.seek(SeekFrom::Start(self.data_start - 4))?;
            self.inner.write_all(&u32::MAX.to_le_bytes())?;
        }
        self.inner.seek(SeekFrom::End(0))?;
        self.inner.flush()?;
        Ok(self.inner)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn reads_the_bwf_time_reference() {
        fn chunk(id: &[u8], body: &[u8]) -> Vec<u8> {
            let mut v = id.to_vec();
            v.extend((body.len() as u32).to_le_bytes());
            v.extend(body);
            v
        }
        let mut bext = vec![0u8; 602];
        bext[338..346].copy_from_slice(&(48_000u64 * 3600).to_le_bytes());
        let mut fmt = Vec::new();
        fmt.extend(1u16.to_le_bytes()); // PCM
        fmt.extend(1u16.to_le_bytes()); // mono
        fmt.extend(48_000u32.to_le_bytes());
        fmt.extend(96_000u32.to_le_bytes());
        fmt.extend(2u16.to_le_bytes());
        fmt.extend(16u16.to_le_bytes());
        let mut body = b"WAVE".to_vec();
        body.extend(chunk(b"bext", &bext));
        body.extend(chunk(b"fmt ", &fmt));
        body.extend(chunk(b"data", &[0u8; 8]));
        let file = chunk(b"RIFF", &body);
        let mut c = io::Cursor::new(file);
        assert_eq!(bext_time_reference(&mut c).unwrap(), Some(48_000 * 3600));
        assert_eq!(read_info(&mut c).unwrap().sample_rate, 48_000);
    }
    use super::*;
    use std::io::Cursor;

    #[test]
    fn broadcast_wave_time_reference_round_trips() {
        let bext = Bext {
            originator: "BananaV".into(),
            originator_reference: "BV-1".into(),
            date: "2026-10-16".into(),
            time: "10:20:30".into(),
            time_reference: 48_000 * 75,
            ..Bext::default()
        };
        let mut w = WavWriter::with_metadata(
            Cursor::new(Vec::new()),
            48_000,
            1,
            SampleFormat::Int(24),
            &[],
            Some(&bext),
        )
        .unwrap();
        w.write_planar(&tone(64, 1)).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let mut c = Cursor::new(&bytes);
        assert_eq!(bext_time_reference(&mut c).unwrap(), Some(48_000 * 75));
        assert_eq!(read_bext(&mut c).unwrap(), Some(bext));
        let mut r = WavReader::new(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(r.info.frames, 64);
        assert_eq!(r.read_planar(0, 64).unwrap()[0].len(), 64);
        assert!(bytes.windows(7).any(|w| w == b"BananaV"));
    }

    #[test]
    fn the_project_s_path_rides_in_a_chunk_of_its_own() {
        let mut w =
            WavWriter::new(Cursor::new(Vec::new()), 48000, 1, SampleFormat::Int(16)).unwrap();
        w.write_planar(&tone(101, 1)).unwrap();
        // An odd length: the chunk is padded.
        w.set_project("C:/Projects/Song.bvproj");
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(
            read_project_reference(&mut Cursor::new(&bytes))
                .unwrap()
                .as_deref(),
            Some("C:/Projects/Song.bvproj")
        );
        // The audio reads as before; the RIFF size holds the chunk.
        let mut r = WavReader::new(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(r.read_planar(0, 101).unwrap()[0].len(), 101);
        let riff = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff + 8, bytes.len());
        // A file without one.
        let mut w =
            WavWriter::new(Cursor::new(Vec::new()), 48000, 1, SampleFormat::Int(16)).unwrap();
        w.write_planar(&tone(10, 1)).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(
            read_project_reference(&mut Cursor::new(&bytes)).unwrap(),
            None
        );
    }

    #[test]
    fn markers_and_regions_are_read_and_rewritten_in_place() {
        let cues = vec![
            Cue {
                frame: 10,
                length: 0,
                name: "Hit".into(),
            },
            Cue {
                frame: 40,
                length: 25,
                name: "Verse".into(),
            },
        ];
        let mut w = WavWriter::with_info(
            Cursor::new(Vec::new()),
            48000,
            1,
            SampleFormat::Int(16),
            &[(*b"INAM", "Song")],
        )
        .unwrap();
        w.write_planar(&tone(101, 1)).unwrap();
        w.set_cues(cues.clone());
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(read_cues(&mut Cursor::new(&bytes)).unwrap(), cues);

        // Rewritten on disk: the audio and its chunks before it stay.
        let path = std::env::temp_dir().join(format!("bv-wav-cues-{}.wav", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let audio = |b: &[u8]| {
            WavReader::new(Cursor::new(b.to_vec()))
                .unwrap()
                .read_planar(0, 101)
                .unwrap()
        };
        let before = audio(&bytes);
        let edited = vec![Cue {
            frame: 5,
            length: 50,
            name: "Intro region".into(),
        }];
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        write_cues(&mut f, &edited).unwrap();
        drop(f);
        let after = std::fs::read(&path).unwrap();
        assert_eq!(read_cues(&mut Cursor::new(&after)).unwrap(), edited);
        assert_eq!(audio(&after), before);
        assert_eq!(after.len() as u32 - 8, u32le(&after[4..8]), "RIFF size");
        // Markers before the audio turn to JUNK; none left.
        let mut moved = bytes.clone();
        let cue_at = moved.windows(4).position(|w| w == b"cue ").unwrap();
        let tail = moved.split_off(cue_at);
        let data = moved.windows(4).position(|w| w == b"data").unwrap();
        let mut early = moved[..data].to_vec();
        early.extend_from_slice(&tail);
        early.extend_from_slice(&moved[data..]);
        let size = early.len() as u32 - 8;
        early[4..8].copy_from_slice(&size.to_le_bytes());
        std::fs::write(&path, &early).unwrap();
        assert_eq!(read_cues(&mut Cursor::new(&early)).unwrap(), cues);
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        write_cues(&mut f, &[]).unwrap();
        drop(f);
        let cleared = std::fs::read(&path).unwrap();
        assert!(read_cues(&mut Cursor::new(&cleared)).unwrap().is_empty());
        assert_eq!(audio(&cleared), before);
        assert_eq!(cleared.len(), early.len(), "nothing moved");
        let _ = std::fs::remove_file(&path);
    }

    fn tone(n: usize, ch: usize) -> Vec<Vec<f32>> {
        (0..ch)
            .map(|c| {
                (0..n)
                    .map(|i| ((i as f32 * 0.05 + c as f32).sin()) * 0.8)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn info_chunk_before_the_data() {
        let mut w = WavWriter::with_info(
            Cursor::new(Vec::new()),
            48000,
            1,
            SampleFormat::Int(16),
            &[(*b"INAM", "Song"), (*b"IART", ""), (*b"ICOP", "(c) 2026")],
        )
        .unwrap();
        w.write_planar(&tone(100, 1)).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let find = |pat: &[u8]| bytes.windows(pat.len()).position(|w| w == pat);
        let list = find(b"LIST").expect("LIST chunk");
        assert!(find(b"INFO") == Some(list + 8));
        assert!(find(b"INAM\x05\0\0\0Song\0").is_some());
        assert!(find(b"ICOP").is_some() && find(b"IART").is_none());
        assert!(list < find(b"data").unwrap());
        // Readers skip it.
        let mut r = WavReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(r.info.frames, 100);
        assert_eq!(r.read_planar(0, 100).unwrap()[0].len(), 100);
    }

    #[test]
    fn ida_e_volta_em_cada_formato() {
        for (format, tol) in [
            (SampleFormat::Int(8), 1.0 / 64.0),
            (SampleFormat::Int(16), 1.0 / 16384.0),
            (SampleFormat::Int(24), 1.0 / 4_000_000.0),
            (SampleFormat::Float(32), 0.0),
        ] {
            let data = tone(1000, 2);
            let mut w = WavWriter::new(Cursor::new(Vec::new()), 48000, 2, format).unwrap();
            w.write_planar(&data[..]).unwrap();
            let bytes = w.finish().unwrap().into_inner();
            let mut r = WavReader::new(Cursor::new(bytes)).unwrap();
            assert_eq!(r.info.frames, 1000);
            assert_eq!(r.info.sample_rate, 48000);
            let back = r.read_planar(0, 1000).unwrap();
            for c in 0..2 {
                for i in 0..1000 {
                    assert!((back[c][i] - data[c][i]).abs() <= tol, "{format:?}");
                }
            }
        }
    }

    #[test]
    fn leitura_parcial_e_fora_dos_limites() {
        let data = tone(100, 1);
        let mut w =
            WavWriter::new(Cursor::new(Vec::new()), 44100, 1, SampleFormat::Float(32)).unwrap();
        w.write_planar(&data).unwrap();
        let mut r = WavReader::new(Cursor::new(w.finish().unwrap().into_inner())).unwrap();
        let part = r.read_planar(-5, 10).unwrap();
        assert!(part[0][..5].iter().all(|&s| s == 0.0));
        assert_eq!(part[0][5..], data[0][..5]);
        let tail = r.read_planar(95, 10).unwrap();
        assert_eq!(tail[0][..5], data[0][95..]);
        assert!(tail[0][5..].iter().all(|&s| s == 0.0));
    }

    /// Arquivos gerados pelo ffmpeg (inclusive WAVE_FORMAT_EXTENSIBLE).
    #[test]
    fn le_arquivos_de_terceiros() {
        for (name, bytes) in [
            ("s16", &include_bytes!("../testdata/tom_s16.wav")[..]),
            ("s24", &include_bytes!("../testdata/tom_s24.wav")[..]),
            ("f32", &include_bytes!("../testdata/tom_f32.wav")[..]),
            ("u8", &include_bytes!("../testdata/tom_u8.wav")[..]),
            ("6ch", &include_bytes!("../testdata/tom_51.wav")[..]),
        ] {
            let mut r =
                WavReader::new(Cursor::new(bytes)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(r.info.sample_rate, 48000, "{name}");
            assert_eq!(r.info.frames, 4800, "{name}");
            let s = r.read_planar(0, 4800).unwrap();
            // O gerador `sine` do ffmpeg tem amplitude 1/8; com volume 0,5,
            // o pico é 1/16.
            let peak = s[0].iter().fold(0f32, |m, &v| m.max(v.abs()));
            assert!((peak - 0.0625).abs() < 0.01, "{name}: pico {peak}");
        }
    }
}
