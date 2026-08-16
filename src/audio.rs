//! 音效播放：直接调用 waveOut*，12 路设备轮转池，支持变调（改采样率）与声像。
//! 移植自 C# TapSoundPlayer.cs（NAudio 逻辑）。

use crate::log;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use windows_sys::Win32::Media::Audio::{
    waveOutClose, waveOutOpen, waveOutPrepareHeader, waveOutReset, waveOutUnprepareHeader,
    waveOutWrite, WAVE_FORMAT_PCM, WAVEHDR, WAVEFORMATEX, WAVE_MAPPER,
};
use windows_sys::Win32::Foundation::HWAVE;

const PLAYER_COUNT: usize = 12;

struct Slot {
    hwo: isize,
    buffer: Vec<u8>,
    header: WAVEHDR,
}

/// 音频播放器：在独立线程上消费播放请求。
pub struct TapPlayer {
    tx: Option<Sender<Request>>,
    handle: Option<thread::JoinHandle<()>>,
    enabled: Arc<std::sync::atomic::AtomicBool>,
    volume: Arc<std::sync::atomic::AtomicU32>, // 0..1000
}

struct Request {
    freq: f64,
    volume: f64,
    balance: f64,
}

impl TapPlayer {
    pub fn new(wav_bytes: Vec<u8>) -> Self {
        let (tx, rx) = mpsc::channel::<Request>();
        let handle = thread::spawn(move || audio_worker(wav_bytes, rx));
        Self {
            tx: Some(tx),
            handle: Some(handle),
            enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            volume: Arc::new(std::sync::atomic::AtomicU32::new(1000)),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_volume(&self, volume: f64) {
        self.volume
            .store((volume.clamp(0.0, 1.0) * 1000.0) as u32, std::sync::atomic::Ordering::Relaxed);
    }

    /// 播放一次。freq 为音调倍率（约 1.0），volume 0..1，balance -1..1。
    pub fn play(&self, freq: f64, volume: f64, balance: f64) {
        if !self.enabled.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if self.volume.load(std::sync::atomic::Ordering::Relaxed) == 0 {
            return;
        }
        if let Some(tx) = &self.tx {
            let _ = tx.send(Request { freq, volume, balance });
        }
    }
}

impl Drop for TapPlayer {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            drop(tx);
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// 解析的 WAV 参数。
struct WaveInfo {
    pcm: Vec<u8>,
    channels: u16,
    sample_rate: u32,
    bits: u16,
}

fn parse_wav(data: &[u8]) -> Option<WaveInfo> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits = 0u16;
    let mut pcm: Vec<u8> = Vec::new();
    let mut offset = 12usize;
    while offset + 8 <= data.len() {
        let id = &data[offset..offset + 4];
        let chunk_size = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().ok()?) as usize;
        let chunk_data = offset + 8;
        if id == b"fmt " {
            let fmt = u16::from_le_bytes(data[chunk_data..chunk_data + 2].try_into().ok()?);
            if fmt != 1 {
                return None;
            }
            channels = u16::from_le_bytes(data[chunk_data + 2..chunk_data + 4].try_into().ok()?);
            sample_rate = u32::from_le_bytes(data[chunk_data + 4..chunk_data + 8].try_into().ok()?);
            bits = u16::from_le_bytes(data[chunk_data + 14..chunk_data + 16].try_into().ok()?);
        } else if id == b"data" {
            pcm = data[chunk_data..chunk_data + chunk_size.min(data.len() - chunk_data)].to_vec();
        }
        offset = chunk_data + chunk_size + (chunk_size & 1);
    }
    if channels == 0 || sample_rate == 0 || bits == 0 || pcm.is_empty() {
        return None;
    }
    Some(WaveInfo { pcm, channels, sample_rate, bits })
}

fn audio_worker(wav_bytes: Vec<u8>, rx: Receiver<Request>) {
    let Some(info) = parse_wav(&wav_bytes) else {
        log("audio: failed to parse WAV");
        return;
    };
    let mut slots: Vec<Box<Slot>> = Vec::with_capacity(PLAYER_COUNT);
    for _ in 0..PLAYER_COUNT {
        let mut hwo: isize = 0;
        let fmt = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM,
            nChannels: info.channels,
            nSamplesPerSec: info.sample_rate,
            nAvgBytesPerSec: info.sample_rate * info.channels as u32 * (info.bits / 8) as u32,
            nBlockAlign: info.channels * (info.bits / 8),
            wBitsPerSample: info.bits,
            cbSize: 0,
        };
        let rc = unsafe { waveOutOpen(&mut hwo, WAVE_MAPPER, &fmt, 0, 0, 0) };
        if rc != 0 {
            break;
        }
        slots.push(Box::new(Slot {
            hwo,
            buffer: Vec::new(),
            header: unsafe { std::mem::zeroed() },
        }));
    }
    if slots.is_empty() {
        log("audio: no waveOut device");
        return;
    }
    let mut next = 0usize;
    while let Ok(req) = rx.recv() {
        let Some(wav) = build_wave(&info, req) else {
            continue;
        };
        let slot = &mut slots[next];
        next = (next + 1) % slots.len();
        unsafe {
            // 清理上一个播放
            if !slot.buffer.is_empty() {
                waveOutReset(slot.hwo);
                waveOutUnprepareHeader(slot.hwo, &mut slot.header, std::mem::size_of::<WAVEHDR>() as u32);
            }
            slot.buffer = wav;
            slot.header = WAVEHDR {
                lpData: slot.buffer.as_mut_ptr() as *mut u16,
                dwBufferLength: slot.buffer.len() as u32,
                dwBytesRecorded: 0,
                dwUser: 0,
                dwFlags: 0,
                dwLoops: 0,
                lpNext: std::ptr::null_mut(),
                reserved: 0,
            };
            waveOutPrepareHeader(slot.hwo, &mut slot.header, std::mem::size_of::<WAVEHDR>() as u32);
            waveOutWrite(slot.hwo, &mut slot.header, std::mem::size_of::<WAVEHDR>() as u32);
        }
    }
    // 清理设备
    for mut slot in slots {
        unsafe {
            if !slot.buffer.is_empty() {
                waveOutReset(slot.hwo);
                waveOutUnprepareHeader(slot.hwo, &mut slot.header, std::mem::size_of::<WAVEHDR>() as u32);
            }
            waveOutClose(slot.hwo);
        }
    }
}

/// 变调 + 增益 + 声像，生成 WAV 字节。
fn build_wave(info: &WaveInfo, req: Request) -> Option<Vec<u8>> {
    let out_rate = (info.sample_rate as f64 * req.freq).round() as u32;
    let out_rate = out_rate.clamp(8000, 96000);
    let bytes_per_sample = (info.bits / 8) as usize;
    let frame_count = info.pcm.len() / (bytes_per_sample * info.channels as usize);
    let mut out = vec![0u8; info.pcm.len()];
    let pan = req.balance.clamp(-1.0, 1.0);
    let left_gain = req.volume * (1.0 - pan).max(0.0).sqrt();
    let right_gain = req.volume * (1.0 + pan).max(0.0).sqrt();
    for i in 0..frame_count {
        let off = i * bytes_per_sample * info.channels as usize;
        if info.channels == 2 {
            let l = read_sample(&info.pcm, off, info.bits);
            let r = read_sample(&info.pcm, off + bytes_per_sample, info.bits);
            write_sample(&mut out, off, l as f64 * left_gain);
            write_sample(&mut out, off + bytes_per_sample, r as f64 * right_gain);
        } else {
            let s = read_sample(&info.pcm, off, info.bits);
            write_sample(&mut out, off, s as f64 * req.volume);
        }
    }
    Some(build_wav_file(&out, out_rate, info.channels, info.bits))
}

fn read_sample(pcm: &[u8], off: usize, bits: u16) -> i16 {
    if bits == 16 {
        i16::from_le_bytes([pcm[off], pcm[off + 1]])
    } else {
        ((pcm[off] as i16 - 128) * 256)
    }
}

fn write_sample(out: &mut [u8], off: usize, value: f64) {
    let v = value.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16;
    out[off] = v as u8;
    out[off + 1] = (v >> 8) as u8;
}

fn build_wav_file(pcm: &[u8], sample_rate: u32, channels: u16, bits: u16) -> Vec<u8> {
    let bytes_per_sample = (bits / 8) as u32;
    let byte_rate = sample_rate * channels as u32 * bytes_per_sample;
    let block_align = channels * bytes_per_sample;
    let mut w = Vec::with_capacity(pcm.len() + 44);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&sample_rate.to_le_bytes());
    w.extend_from_slice(&byte_rate.to_le_bytes());
    w.extend_from_slice(&block_align.to_le_bytes());
    w.extend_from_slice(&bits.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(pcm);
    w
}