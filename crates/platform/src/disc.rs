//! Burning discs (Tools › Burn Disc): the computer's drives that write
//! discs; an image file (a disc's sectors, `bv_dvd`'s) written onto the
//! blank disc in one of them and closed, so players read it; and audio
//! CDs — a track added at a time (Track-at-Once) or a whole disc of
//! tracks with their indices, ISRCs and flags (Disc-at-Once, Red Book),
//! the disc closed, erased or ejected as asked — through the system's
//! recording service (IMAPI2 on Windows; none elsewhere yet).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// A drive that writes discs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Burner {
    /// The system's id of it.
    pub id: String,
    /// Its letter and model, as lists show it ("E: ASUS DRW-24D5MT").
    pub name: String,
}

/// Why a disc was not burned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BurnError {
    /// The drive is gone, or it cannot write discs.
    Drive,
    /// No disc the drive can write is in it.
    NoDisc,
    /// The disc has something on it already.
    NotBlank,
    /// The image needs `needed` sectors and the disc has `free`.
    TooBig {
        needed: u64,
        free: u64,
    },
    Canceled,
    /// What the system said.
    Failed(String),
}

/// A DVD's speed (1×: 1,385,000 bytes a second) in the 2048-byte sectors
/// a second the system counts.
pub const DVD_1X: u32 = 680;

/// A CD's speed (1×: 75 sectors of 2352 bytes a second, CD audio's).
pub const CD_1X: u32 = 75;

/// An audio CD's sector: 1/75 s of 16-bit stereo at 44.1 kHz.
pub const CD_SECTOR: u64 = 2352;

/// How an audio CD is burned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CdOptions {
    /// Times a CD's speed (`None`: as fast as the drive goes).
    pub speed: Option<u32>,
    pub eject: bool,
    /// Buffer underrun protection: the drive may stop and resume.
    pub underrun_protection: bool,
    /// A rewritable disc is erased first.
    pub erase: bool,
}

/// A track of a Disc-at-Once CD: its sound — CD audio, 16-bit stereo at
/// 44.1 kHz, little-endian — `len` bytes from `offset` of `file`, and
/// what its subcode says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CdTrackData {
    pub file: PathBuf,
    pub offset: u64,
    pub len: u64,
    /// Where its indices after the first begin, in sectors from its start.
    pub indices: Vec<u32>,
    /// Its International Standard Recording Code (empty: none).
    pub isrc: String,
    /// Digital copies prohibited.
    pub protect: bool,
    /// Its sound has pre-emphasis.
    pub emphasis: bool,
}

/// The drives that write discs, by their letters (none where the system
/// has no recording service).
pub fn burners() -> Vec<Burner> {
    imp::burners()
}

/// The sectors free on the disc in `burner` (`None`: no disc it can
/// write).
pub fn free_sectors(burner: &str) -> Option<u64> {
    imp::free_sectors(burner)
}

/// Writes the image file onto the blank disc in the drive `burner` (a
/// [`Burner::id`]) at `speed` times a DVD's (`None`: as fast as the drive
/// goes), the disc closed so players read it, and ejects it when `eject`.
/// `progress` hears how much of the image went to the drive (0–1), from
/// the system's threads; `cancel` stops it.
pub fn burn_image(
    burner: &str,
    image: &Path,
    speed: Option<u32>,
    eject: bool,
    progress: Arc<dyn Fn(f32) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> Result<(), BurnError> {
    imp::burn_image(burner, image, speed, eject, progress, cancel)
}

/// Track-at-Once: `len` bytes of CD audio from `offset` of `file` added
/// to the CD in `burner` as one track (two seconds of pause before it),
/// the disc closed after it when `close` (players read only closed
/// discs).
#[allow(clippy::too_many_arguments)]
pub fn burn_audio_track(
    burner: &str,
    file: &Path,
    offset: u64,
    len: u64,
    close: bool,
    options: CdOptions,
    progress: Arc<dyn Fn(f32) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> Result<(), BurnError> {
    imp::burn_audio_track(burner, file, offset, len, close, options, progress, cancel)
}

/// Disc-at-Once: the tracks written in one go, numbered from
/// `first_track`, with the disc's catalog number (UPC/EAN; empty: none).
pub fn burn_audio_disc(
    burner: &str,
    tracks: &[CdTrackData],
    first_track: u32,
    catalog: &str,
    options: CdOptions,
    progress: Arc<dyn Fn(f32) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> Result<(), BurnError> {
    imp::burn_audio_disc(
        burner,
        tracks,
        first_track,
        catalog,
        options,
        progress,
        cancel,
    )
}

/// Closes the CD in `burner` (Track-at-Once's Close disc) and ejects it
/// when `eject`.
pub fn close_disc(burner: &str, eject: bool) -> Result<(), BurnError> {
    imp::close_disc(burner, eject)
}

/// Erases the rewritable disc in `burner` (quickly).
pub fn erase_disc(burner: &str) -> Result<(), BurnError> {
    imp::erase_disc(burner)
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use std::ffi::c_void;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use windows::Win32::Foundation::{
        E_ABORT, E_FAIL, E_NOTIMPL, E_POINTER, S_FALSE, S_OK, STG_E_ACCESSDENIED,
        STG_E_INVALIDFUNCTION, VARIANT_BOOL, VARIANT_FALSE, VARIANT_TRUE,
    };
    use windows::Win32::Storage::FileSystem::GetVolumePathNamesForVolumeNameW;
    use windows::Win32::Storage::Imapi::{
        IDiscFormat2, IDiscFormat2Data, IDiscFormat2Erase, IDiscFormat2RawCD,
        IDiscFormat2TrackAtOnce, IDiscMaster2, IDiscRecorder2, IMAPI_CD_SECTOR_AUDIO,
        IMAPI_CD_TRACK_DIGITAL_COPY_PROHIBITED, IMAPI_FORMAT2_RAW_CD_SUBCODE_IS_RAW,
        IRawCDImageCreator, MsftDiscFormat2Data, MsftDiscFormat2Erase, MsftDiscFormat2RawCD,
        MsftDiscFormat2TrackAtOnce, MsftDiscMaster2, MsftDiscRecorder2, MsftRawCDImageCreator,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
        ISequentialStream_Impl, IStream, IStream_Impl, LOCKTYPE, STATFLAG, STATSTG, STGC,
        STGTY_STREAM, STREAM_SEEK, STREAM_SEEK_CUR, STREAM_SEEK_END, STREAM_SEEK_SET,
    };
    use windows::core::{BSTR, Error, GUID, HRESULT, Interface, PCWSTR, Ref, Result, implement};

    use super::{BurnError, Burner, CD_1X, CD_SECTOR, CdOptions, CdTrackData, DVD_1X};

    /// `f` with COM on this thread, given back after when this call
    /// started it.
    fn with_com<T>(f: impl FnOnce() -> T) -> T {
        // SAFETY: plain COM start and end on this thread; S_FALSE (already
        // started here) is undone too, RPC_E_CHANGED_MODE is not.
        let started = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let out = f();
        if started {
            unsafe { CoUninitialize() };
        }
        out
    }

    fn yes(b: VARIANT_BOOL) -> bool {
        b.0 != 0
    }

    fn flag(on: bool) -> VARIANT_BOOL {
        if on { VARIANT_TRUE } else { VARIANT_FALSE }
    }

    fn failed(e: Error) -> BurnError {
        BurnError::Failed(e.message())
    }

    /// One of the recording service's objects.
    fn create<T: Interface>(class: &GUID) -> std::result::Result<T, BurnError> {
        // SAFETY: a COM object of the system's recording service.
        unsafe { CoCreateInstance(class, None, CLSCTX_ALL) }.map_err(|_| BurnError::Drive)
    }

    /// The drive `id`, ready to be asked.
    fn recorder(id: &BSTR) -> Option<IDiscRecorder2> {
        let r: IDiscRecorder2 = create(&MsftDiscRecorder2).ok()?;
        // SAFETY: the recorder's own method, given its id.
        unsafe { r.InitializeDiscRecorder(id) }.ok()?;
        Some(r)
    }

    /// Whether the drive can write the format's discs, and holds one.
    fn check(format: &IDiscFormat2, r: &IDiscRecorder2) -> std::result::Result<(), BurnError> {
        // SAFETY: the format's own methods, given a live recorder.
        unsafe {
            if !format.IsRecorderSupported(r).is_ok_and(yes) {
                return Err(BurnError::Drive);
            }
            if !format.IsCurrentMediaSupported(r).is_ok_and(yes) {
                return Err(BurnError::NoDisc);
            }
        }
        Ok(())
    }

    /// The drive's letter ("E:"), from its volume's paths.
    fn letter(r: &IDiscRecorder2) -> Option<String> {
        // SAFETY: the volume's name as the service gives it, and a buffer
        // the call fills (its length in characters given).
        unsafe {
            let volume = r.VolumeName().ok()?;
            let wide: Vec<u16> = volume.iter().copied().chain([0]).collect();
            let mut buf = [0u16; 260];
            let mut len = 0u32;
            GetVolumePathNamesForVolumeNameW(PCWSTR(wide.as_ptr()), Some(&mut buf), &mut len)
                .ok()?;
            let end = buf.iter().position(|&c| c == 0).unwrap_or(0);
            let path = String::from_utf16_lossy(&buf[..end]);
            let letter = path.trim_end_matches('\\').to_string();
            (!letter.is_empty()).then_some(letter)
        }
    }

    pub fn burners() -> Vec<Burner> {
        with_com(|| {
            let Ok(master) = create::<IDiscMaster2>(&MsftDiscMaster2) else {
                return Vec::new();
            };
            let Ok(format) = create::<IDiscFormat2Data>(&MsftDiscFormat2Data) else {
                return Vec::new();
            };
            // SAFETY: the service's objects, asked through their
            // interfaces.
            unsafe {
                if !master.IsSupportedEnvironment().is_ok_and(yes) {
                    return Vec::new();
                }
                let count = master.Count().unwrap_or(0);
                let mut out = Vec::new();
                for k in 0..count {
                    let Ok(id) = master.get_Item(k) else {
                        continue;
                    };
                    let Some(r) = recorder(&id) else {
                        continue;
                    };
                    if !format.IsRecorderSupported(&r).is_ok_and(yes) {
                        continue;
                    }
                    let text = |b: Result<BSTR>| b.map(|b| b.to_string()).unwrap_or_default();
                    let parts = [
                        letter(&r).unwrap_or_default(),
                        text(r.VendorId()).trim().to_string(),
                        text(r.ProductId()).trim().to_string(),
                    ];
                    let name = parts
                        .iter()
                        .filter(|p| !p.is_empty())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ");
                    out.push(Burner {
                        id: id.to_string(),
                        name,
                    });
                }
                out
            }
        })
    }

    pub fn free_sectors(burner: &str) -> Option<u64> {
        with_com(|| {
            let r = recorder(&BSTR::from(burner))?;
            let format: IDiscFormat2Data = create(&MsftDiscFormat2Data).ok()?;
            // SAFETY: the format's own methods, given a live recorder.
            unsafe {
                if !format.IsCurrentMediaSupported(&r).is_ok_and(yes) {
                    return None;
                }
                format.SetRecorder(&r).ok()?;
                format.FreeSectorsOnMedia().ok().map(|n| n.max(0) as u64)
            }
        })
    }

    /// The bytes read from all of a burn's streams, told to `progress` as
    /// a share of `total`.
    fn counter(
        total: u64,
        progress: Arc<dyn Fn(f32) + Send + Sync>,
    ) -> Arc<dyn Fn(u64) + Send + Sync> {
        let read = AtomicU64::new(0);
        Arc::new(move |n| {
            let r = read.fetch_add(n, Ordering::Relaxed) + n;
            progress((r as f64 / total.max(1) as f64).min(1.0) as f32);
        })
    }

    fn file_len(path: &Path) -> std::result::Result<u64, BurnError> {
        std::fs::metadata(path)
            .map(|m| m.len())
            .map_err(|e| BurnError::Failed(e.to_string()))
    }

    pub fn burn_image(
        burner: &str,
        image: &Path,
        speed: Option<u32>,
        eject: bool,
        progress: Arc<dyn Fn(f32) + Send + Sync>,
        cancel: Arc<AtomicBool>,
    ) -> std::result::Result<(), BurnError> {
        let size = file_len(image)?;
        with_com(|| {
            let r = recorder(&BSTR::from(burner)).ok_or(BurnError::Drive)?;
            let format: IDiscFormat2Data = create(&MsftDiscFormat2Data)?;
            check(&format, &r)?;
            // SAFETY: the service's objects, asked through their
            // interfaces; the stream is BananaV's own.
            unsafe {
                format.SetRecorder(&r).map_err(failed)?;
                format
                    .SetClientName(&BSTR::from("BananaV"))
                    .map_err(failed)?;
                if !format.MediaHeuristicallyBlank().is_ok_and(yes) {
                    return Err(BurnError::NotBlank);
                }
                let free = format.FreeSectorsOnMedia().map_err(failed)?.max(0) as u64;
                let needed = size.div_ceil(2048);
                if needed > free {
                    return Err(BurnError::TooBig { needed, free });
                }
                format
                    .SetForceMediaToBeClosed(VARIANT_TRUE)
                    .map_err(failed)?;
                // -1: the drive's fastest.
                let sectors = speed.map_or(-1, |x| (x * DVD_1X) as i32);
                let _ = format.SetWriteSpeed(sectors, VARIANT_FALSE);
                let stream =
                    ImageStream::open(image, 0, size, 1, counter(size, progress), cancel.clone())?;
                let written = format.Write(&stream);
                if cancel.load(Ordering::Relaxed) {
                    return Err(BurnError::Canceled);
                }
                written.map_err(failed)?;
                if eject {
                    let _ = r.EjectMedia();
                }
                Ok(())
            }
        })
    }

    /// Erases the disc in `r` (quickly).
    fn erase(r: &IDiscRecorder2) -> std::result::Result<(), BurnError> {
        let e: IDiscFormat2Erase = create(&MsftDiscFormat2Erase)?;
        // SAFETY: the eraser's own methods, given a live recorder.
        unsafe {
            e.SetRecorder(r).map_err(failed)?;
            e.SetClientName(&BSTR::from("BananaV")).map_err(failed)?;
            e.SetFullErase(VARIANT_FALSE).map_err(failed)?;
            e.EraseMedia().map_err(failed)
        }
    }

    pub fn erase_disc(burner: &str) -> std::result::Result<(), BurnError> {
        with_com(|| {
            let r = recorder(&BSTR::from(burner)).ok_or(BurnError::Drive)?;
            erase(&r)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn burn_audio_track(
        burner: &str,
        file: &Path,
        offset: u64,
        len: u64,
        close: bool,
        options: CdOptions,
        progress: Arc<dyn Fn(f32) + Send + Sync>,
        cancel: Arc<AtomicBool>,
    ) -> std::result::Result<(), BurnError> {
        with_com(|| {
            let r = recorder(&BSTR::from(burner)).ok_or(BurnError::Drive)?;
            if options.erase {
                let _ = erase(&r);
            }
            let tao: IDiscFormat2TrackAtOnce = create(&MsftDiscFormat2TrackAtOnce)?;
            check(&tao, &r)?;
            // SAFETY: the service's objects, asked through their
            // interfaces; the stream is BananaV's own.
            unsafe {
                tao.SetRecorder(&r).map_err(failed)?;
                tao.SetClientName(&BSTR::from("BananaV")).map_err(failed)?;
                tao.SetBufferUnderrunFreeDisabled(flag(!options.underrun_protection))
                    .map_err(failed)?;
                tao.SetDoNotFinalizeMedia(flag(!close)).map_err(failed)?;
                let sectors = options.speed.map_or(-1, |x| (x * CD_1X) as i32);
                let _ = tao.SetWriteSpeed(sectors, VARIANT_FALSE);
                tao.PrepareMedia().map_err(failed)?;
                let result = (|| {
                    // The track and its two seconds of pause.
                    let free = tao.FreeSectorsOnMedia().map_err(failed)?.max(0) as u64;
                    let needed = len.div_ceil(CD_SECTOR) + 150;
                    if needed > free {
                        return Err(BurnError::TooBig { needed, free });
                    }
                    let stream = ImageStream::open(
                        file,
                        offset,
                        len,
                        CD_SECTOR,
                        counter(len, progress),
                        cancel.clone(),
                    )?;
                    tao.AddAudioTrack(&stream).map_err(failed)
                })();
                let released = tao.ReleaseMedia();
                if cancel.load(Ordering::Relaxed) {
                    return Err(BurnError::Canceled);
                }
                result?;
                released.map_err(failed)?;
                if options.eject {
                    let _ = r.EjectMedia();
                }
                Ok(())
            }
        })
    }

    pub fn close_disc(burner: &str, eject: bool) -> std::result::Result<(), BurnError> {
        with_com(|| {
            let r = recorder(&BSTR::from(burner)).ok_or(BurnError::Drive)?;
            let tao: IDiscFormat2TrackAtOnce = create(&MsftDiscFormat2TrackAtOnce)?;
            check(&tao, &r)?;
            // SAFETY: the format's own methods, given a live recorder.
            unsafe {
                tao.SetRecorder(&r).map_err(failed)?;
                tao.SetClientName(&BSTR::from("BananaV")).map_err(failed)?;
                tao.SetDoNotFinalizeMedia(VARIANT_FALSE).map_err(failed)?;
                tao.PrepareMedia().map_err(failed)?;
                tao.ReleaseMedia().map_err(failed)?;
                if eject {
                    let _ = r.EjectMedia();
                }
            }
            Ok(())
        })
    }

    /// The Disc-at-Once image of the tracks: the service's raw sectors
    /// (with their subcode), and how many bytes of sound they read.
    pub(super) fn dao_image(
        tracks: &[CdTrackData],
        first_track: u32,
        catalog: &str,
        progress: Arc<dyn Fn(f32) + Send + Sync>,
        cancel: &Arc<AtomicBool>,
    ) -> std::result::Result<IStream, BurnError> {
        let creator: IRawCDImageCreator = create(&MsftRawCDImageCreator)?;
        let total = tracks.iter().map(|t| t.len).sum();
        let count = counter(total, progress);
        // SAFETY: the creator's own methods; the streams are BananaV's.
        unsafe {
            creator
                .SetResultingImageType(IMAPI_FORMAT2_RAW_CD_SUBCODE_IS_RAW)
                .map_err(failed)?;
            creator
                .SetStartingTrackNumber(first_track.clamp(1, 99) as i32)
                .map_err(failed)?;
            if !catalog.trim().is_empty() {
                creator
                    .SetMediaCatalogNumber(&BSTR::from(catalog.trim()))
                    .map_err(failed)?;
            }
            // The pauses are in the tracks' sound already.
            creator
                .SetDisableGaplessAudio(VARIANT_FALSE)
                .map_err(failed)?;
            for t in tracks {
                let stream = ImageStream::open(
                    &t.file,
                    t.offset,
                    t.len,
                    CD_SECTOR,
                    count.clone(),
                    cancel.clone(),
                )?;
                let at = creator
                    .AddTrack(IMAPI_CD_SECTOR_AUDIO, &stream)
                    .map_err(failed)?;
                let info = creator.get_TrackInfo(at).map_err(failed)?;
                if !t.isrc.trim().is_empty() {
                    info.SetISRC(&BSTR::from(t.isrc.trim())).map_err(failed)?;
                }
                if t.protect {
                    info.SetDigitalAudioCopySetting(IMAPI_CD_TRACK_DIGITAL_COPY_PROHIBITED)
                        .map_err(failed)?;
                }
                if t.emphasis {
                    info.SetAudioHasPreemphasis(VARIANT_TRUE).map_err(failed)?;
                }
                for &i in &t.indices {
                    info.AddTrackIndex(i as i32).map_err(failed)?;
                }
            }
            creator.CreateResultImage().map_err(failed)
        }
    }

    pub fn burn_audio_disc(
        burner: &str,
        tracks: &[CdTrackData],
        first_track: u32,
        catalog: &str,
        options: CdOptions,
        progress: Arc<dyn Fn(f32) + Send + Sync>,
        cancel: Arc<AtomicBool>,
    ) -> std::result::Result<(), BurnError> {
        with_com(|| {
            let r = recorder(&BSTR::from(burner)).ok_or(BurnError::Drive)?;
            if options.erase {
                let _ = erase(&r);
            }
            let raw: IDiscFormat2RawCD = create(&MsftDiscFormat2RawCD)?;
            check(&raw, &r)?;
            let image = dao_image(tracks, first_track, catalog, progress, &cancel)?;
            // SAFETY: the service's objects, asked through their
            // interfaces.
            unsafe {
                raw.SetRecorder(&r).map_err(failed)?;
                raw.SetClientName(&BSTR::from("BananaV")).map_err(failed)?;
                raw.SetRequestedSectorType(IMAPI_FORMAT2_RAW_CD_SUBCODE_IS_RAW)
                    .map_err(failed)?;
                raw.SetBufferUnderrunFreeDisabled(flag(!options.underrun_protection))
                    .map_err(failed)?;
                let sectors = options.speed.map_or(-1, |x| (x * CD_1X) as i32);
                let _ = raw.SetWriteSpeed(sectors, VARIANT_FALSE);
                raw.PrepareMedia().map_err(failed)?;
                let written = raw.WriteMedia(&image);
                let released = raw.ReleaseMedia();
                if cancel.load(Ordering::Relaxed) {
                    return Err(BurnError::Canceled);
                }
                written.map_err(failed)?;
                released.map_err(failed)?;
                if options.eject {
                    let _ = r.EjectMedia();
                }
            }
            Ok(())
        })
    }

    /// A part of a file as the stream the service reads — `len` bytes
    /// from `start`, then zeros to a whole number of `unit`s (a CD's
    /// sectors) —, telling how much it read and failing once canceled
    /// (the service then stops).
    #[implement(IStream)]
    pub(super) struct ImageStream {
        file: Mutex<File>,
        start: u64,
        len: u64,
        size: u64,
        /// Where the next read starts, in the stream.
        pos: Mutex<u64>,
        read: Arc<dyn Fn(u64) + Send + Sync>,
        cancel: Arc<AtomicBool>,
    }

    impl ImageStream {
        pub(super) fn open(
            path: &Path,
            start: u64,
            len: u64,
            unit: u64,
            read: Arc<dyn Fn(u64) + Send + Sync>,
            cancel: Arc<AtomicBool>,
        ) -> std::result::Result<IStream, BurnError> {
            let file = File::open(path).map_err(|e| BurnError::Failed(e.to_string()))?;
            Ok(ImageStream {
                file: Mutex::new(file),
                start,
                len,
                size: len.div_ceil(unit.max(1)) * unit.max(1),
                pos: Mutex::new(0),
                read,
                cancel,
            }
            .into())
        }
    }

    impl ISequentialStream_Impl for ImageStream_Impl {
        fn Read(&self, pv: *mut c_void, cb: u32, pcbread: *mut u32) -> HRESULT {
            if self.cancel.load(Ordering::Relaxed) {
                return E_ABORT;
            }
            if pv.is_null() {
                return E_POINTER;
            }
            let (Ok(mut file), Ok(mut pos)) = (self.file.lock(), self.pos.lock()) else {
                return E_FAIL;
            };
            // SAFETY: the caller gives `cb` bytes to fill at `pv`.
            let buf = unsafe { std::slice::from_raw_parts_mut(pv.cast::<u8>(), cb as usize) };
            let want = (cb as u64).min(self.size.saturating_sub(*pos)) as usize;
            // The file's part, then the zeros after it.
            let from_file = self.len.saturating_sub(*pos).min(want as u64) as usize;
            if from_file > 0
                && (file.seek(SeekFrom::Start(self.start + *pos)).is_err()
                    || file.read_exact(&mut buf[..from_file]).is_err())
            {
                return E_FAIL;
            }
            buf[from_file..want].fill(0);
            *pos += want as u64;
            if !pcbread.is_null() {
                // SAFETY: the caller's place for the count.
                unsafe { *pcbread = want as u32 };
            }
            (self.read)(want as u64);
            if want < cb as usize { S_FALSE } else { S_OK }
        }

        fn Write(&self, _: *const c_void, _: u32, _: *mut u32) -> HRESULT {
            STG_E_ACCESSDENIED
        }
    }

    impl IStream_Impl for ImageStream_Impl {
        fn Seek(
            &self,
            dlibmove: i64,
            dworigin: STREAM_SEEK,
            plibnewposition: *mut u64,
        ) -> Result<()> {
            let mut pos = self.pos.lock().map_err(|_| Error::from(E_FAIL))?;
            let base = match dworigin {
                STREAM_SEEK_SET => 0,
                STREAM_SEEK_CUR => *pos as i64,
                STREAM_SEEK_END => self.size as i64,
                _ => return Err(STG_E_INVALIDFUNCTION.into()),
            };
            let to = base + dlibmove;
            if to < 0 {
                return Err(STG_E_INVALIDFUNCTION.into());
            }
            *pos = to as u64;
            if !plibnewposition.is_null() {
                // SAFETY: the caller's place for the new position.
                unsafe { *plibnewposition = *pos };
            }
            Ok(())
        }

        fn SetSize(&self, _: u64) -> Result<()> {
            Err(STG_E_ACCESSDENIED.into())
        }

        fn CopyTo(&self, _: Ref<'_, IStream>, _: u64, _: *mut u64, _: *mut u64) -> Result<()> {
            Err(E_NOTIMPL.into())
        }

        fn Commit(&self, _: &STGC) -> Result<()> {
            Ok(())
        }

        fn Revert(&self) -> Result<()> {
            Ok(())
        }

        fn LockRegion(&self, _: u64, _: u64, _: &LOCKTYPE) -> Result<()> {
            Err(STG_E_INVALIDFUNCTION.into())
        }

        fn UnlockRegion(&self, _: u64, _: u64, _: u32) -> Result<()> {
            Err(STG_E_INVALIDFUNCTION.into())
        }

        fn Stat(&self, pstatstg: *mut STATSTG, _: &STATFLAG) -> Result<()> {
            if pstatstg.is_null() {
                return Err(E_POINTER.into());
            }
            // SAFETY: the caller's structure to fill.
            unsafe {
                *pstatstg = STATSTG {
                    r#type: STGTY_STREAM.0 as u32,
                    cbSize: self.size,
                    ..STATSTG::default()
                };
            }
            Ok(())
        }

        fn Clone(&self) -> Result<IStream> {
            Err(E_NOTIMPL.into())
        }
    }

    #[cfg(test)]
    mod tests {
        use std::sync::atomic::AtomicU32;

        use windows::Win32::System::Com::STATFLAG_NONAME;

        use super::*;

        #[test]
        fn the_service_reads_a_file_s_part_as_a_stream() {
            let path = std::env::temp_dir().join(format!("bv-burn-{}.iso", std::process::id()));
            let bytes: Vec<u8> = (0..5000u32).map(|k| (k % 251) as u8).collect();
            std::fs::write(&path, &bytes).unwrap();
            let heard = Arc::new(AtomicU32::new(0));
            let h = heard.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            // From byte 100, 4000 bytes, in sectors of 2352: 4704.
            let stream = ImageStream::open(
                &path,
                100,
                4000,
                CD_SECTOR,
                counter(
                    4704,
                    Arc::new(move |p| h.store(p.to_bits(), Ordering::Relaxed)),
                ),
                cancel.clone(),
            )
            .unwrap();
            // SAFETY: the stream's own methods, with buffers of the sizes
            // given.
            unsafe {
                let mut stat = STATSTG::default();
                stream.Stat(&mut stat, STATFLAG_NONAME).unwrap();
                assert_eq!(stat.cbSize, 4704);
                let mut buf = vec![7u8; 4096];
                let mut n = 0u32;
                let hr = stream.Read(buf.as_mut_ptr().cast(), 4096, Some(&mut n));
                assert_eq!((hr, n), (S_OK, 4096));
                assert_eq!(buf[..4000], bytes[100..4100]);
                assert_eq!(&buf[4000..], [0; 96], "zeros past the part");
                let hr = stream.Read(buf.as_mut_ptr().cast(), 4096, Some(&mut n));
                assert_eq!((hr, n), (S_FALSE, 608));
                assert_eq!(f32::from_bits(heard.load(Ordering::Relaxed)), 1.0);
                let mut at = 0u64;
                stream.Seek(10, STREAM_SEEK_SET, Some(&mut at)).unwrap();
                assert_eq!(at, 10);
                stream
                    .Read(buf.as_mut_ptr().cast(), 1, Some(&mut n))
                    .ok()
                    .unwrap();
                assert_eq!(buf[0], bytes[110]);
                cancel.store(true, Ordering::Relaxed);
                let hr = stream.Read(buf.as_mut_ptr().cast(), 1, Some(&mut n));
                assert_eq!(hr, E_ABORT);
            }
            drop(stream);
            let _ = std::fs::remove_file(&path);
        }

        #[test]
        fn a_disc_at_once_image_holds_its_tracks() {
            // Two tracks of CD audio (4 s each, Red Book's shortest), the
            // second with an index a second in: the service's own image of
            // them, made without a drive.
            let path = std::env::temp_dir().join(format!("bv-dao-{}.pcm", std::process::id()));
            let second = 44_100 * 4;
            std::fs::write(&path, vec![1u8; 8 * second]).unwrap();
            let track = |offset: usize, len: usize, indices: Vec<u32>| CdTrackData {
                file: path.clone(),
                offset: offset as u64,
                len: len as u64,
                indices,
                isrc: String::new(),
                protect: false,
                emphasis: false,
            };
            let tracks = [
                track(0, 4 * second, Vec::new()),
                track(4 * second, 4 * second, vec![75]),
            ];
            let result = with_com(|| {
                let image = dao_image(
                    &tracks,
                    1,
                    "",
                    Arc::new(|_| {}),
                    &Arc::new(AtomicBool::new(false)),
                );
                // SAFETY: the image stream's own method.
                image.map(|s| unsafe {
                    let mut stat = STATSTG::default();
                    s.Stat(&mut stat, STATFLAG_NONAME).map(|_| stat.cbSize)
                })
            });
            let _ = std::fs::remove_file(&path);
            match result {
                // Raw sectors with their subcode (2448 bytes): what the
                // service adds (the pregap), then 300 + 300 sectors.
                Ok(Ok(size)) => {
                    assert_eq!(size % 2448, 0, "{size}");
                    assert!(size / 2448 >= 600, "{size}");
                }
                // Systems without the service's image creator.
                Ok(Err(e)) => panic!("{e:?}"),
                Err(BurnError::Drive) => {}
                Err(e) => panic!("{e:?}"),
            }
        }

        #[test]
        fn the_drives_are_listed_by_name() {
            // Whatever the computer has (often none): named, never a
            // failure.
            for b in burners() {
                assert!(!b.id.is_empty() && !b.name.is_empty(), "{b:?}");
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use super::{BurnError, Burner, CdOptions, CdTrackData};

    pub fn burners() -> Vec<Burner> {
        Vec::new()
    }

    pub fn free_sectors(_: &str) -> Option<u64> {
        None
    }

    pub fn burn_image(
        _: &str,
        _: &Path,
        _: Option<u32>,
        _: bool,
        _: Arc<dyn Fn(f32) + Send + Sync>,
        _: Arc<AtomicBool>,
    ) -> Result<(), BurnError> {
        Err(BurnError::Drive)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn burn_audio_track(
        _: &str,
        _: &Path,
        _: u64,
        _: u64,
        _: bool,
        _: CdOptions,
        _: Arc<dyn Fn(f32) + Send + Sync>,
        _: Arc<AtomicBool>,
    ) -> Result<(), BurnError> {
        Err(BurnError::Drive)
    }

    pub fn burn_audio_disc(
        _: &str,
        _: &[CdTrackData],
        _: u32,
        _: &str,
        _: CdOptions,
        _: Arc<dyn Fn(f32) + Send + Sync>,
        _: Arc<AtomicBool>,
    ) -> Result<(), BurnError> {
        Err(BurnError::Drive)
    }

    pub fn close_disc(_: &str, _: bool) -> Result<(), BurnError> {
        Err(BurnError::Drive)
    }

    pub fn erase_disc(_: &str) -> Result<(), BurnError> {
        Err(BurnError::Drive)
    }
}
