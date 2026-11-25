//! Audio capture, IMA-ADPCM 4:1 compression, and WASAPI playback (UU feature:
//! voice_setting / device_microphone / remote_mute / audio).
//!
//! Wire payload for `FrameType::Audio`:
//!   [4B sample_rate BE][1B channels][1B codec (0=PCM16, 1=IMA-ADPCM)][2B samples_per_ch BE][data]

use anyhow::{bail, ensure, Context, Result};
use crate::frame::{Frame, FrameType};

pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
pub const AUDIO_CHANNELS: u8 = 2;

const AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM: u32 = 0x8000_0000;
const AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AudioCodec {
    Pcm16 = 0,
    ImaAdpcm = 1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioPacket {
    pub sample_rate: u32,
    pub channels: u8,
    pub codec: AudioCodec,
    pub samples_per_ch: u16,
    pub data: Vec<u8>,
}

impl AudioPacket {
    pub const HDR_LEN: usize = 8;

    /// Encode PCM16 interleaved samples into an IMA-ADPCM compressed AudioPacket.
    pub fn encode_adpcm(sample_rate: u32, channels: u8, pcm: &[i16]) -> Result<Self> {
        ensure!(channels == 1 || channels == 2, "unsupported channel count {channels}");
        ensure!(pcm.len() % channels as usize == 0, "unaligned pcm samples");
        let samples_per_ch = (pcm.len() / channels as usize) as u16;
        let data = adpcm_encode(pcm, channels as usize);
        Ok(Self {
            sample_rate,
            channels,
            codec: AudioCodec::ImaAdpcm,
            samples_per_ch,
            data,
        })
    }

    /// Decode back to interleaved i16 PCM samples.
    pub fn decode_pcm(&self) -> Result<Vec<i16>> {
        match self.codec {
            AudioCodec::Pcm16 => {
                ensure!(self.data.len() % 2 == 0, "odd pcm16 bytes");
                Ok(self
                    .data
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect())
            }
            AudioCodec::ImaAdpcm => {
                adpcm_decode(&self.data, self.channels as usize, self.samples_per_ch as usize)
            }
        }
    }

    pub fn to_wire(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(Self::HDR_LEN + self.data.len());
        out.extend_from_slice(&self.sample_rate.to_be_bytes());
        out.push(self.channels);
        out.push(self.codec as u8);
        out.extend_from_slice(&self.samples_per_ch.to_be_bytes());
        out.extend_from_slice(&self.data);
        out
    }

    pub fn from_wire(b: &[u8]) -> Result<Self> {
        ensure!(b.len() >= Self::HDR_LEN, "short audio payload");
        let sample_rate = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        let channels = b[4];
        let codec = match b[5] {
            0 => AudioCodec::Pcm16,
            1 => AudioCodec::ImaAdpcm,
            c => bail!("unknown audio codec {c}"),
        };
        let samples_per_ch = u16::from_be_bytes([b[6], b[7]]);
        Ok(Self {
            sample_rate,
            channels,
            codec,
            samples_per_ch,
            data: b[Self::HDR_LEN..].to_vec(),
        })
    }

    pub fn into_frame(&self, seq: u32) -> Frame {
        Frame::new(FrameType::Audio, 4, seq, self.to_wire())
    }
}

// ---------------------------------------------------------------------------
// Pure-Rust IMA-ADPCM 4:1 codec (zero external C/libopus toolchain required)
// ---------------------------------------------------------------------------

const INDEX_TABLE: [i8; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];
const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

#[derive(Clone, Copy, Default)]
struct AdpcmState {
    predictor: i32,
    step_index: i32,
}

impl AdpcmState {
    fn encode_nibble(&mut self, sample: i16) -> u8 {
        let step = STEP_TABLE[self.step_index as usize];
        let mut diff = sample as i32 - self.predictor;
        let mut code: u8 = 0;
        if diff < 0 {
            code = 8;
            diff = -diff;
        }
        let mut delta_step = step >> 3;
        if diff >= step {
            code |= 4;
            diff -= step;
            delta_step += step;
        }
        if diff >= (step >> 1) {
            code |= 2;
            diff -= step >> 1;
            delta_step += step >> 1;
        }
        if diff >= (step >> 2) {
            code |= 1;
            delta_step += step >> 2;
        }
        if (code & 8) != 0 {
            self.predictor -= delta_step;
        } else {
            self.predictor += delta_step;
        }
        self.predictor = self.predictor.clamp(-32768, 32767);
        self.step_index = (self.step_index + INDEX_TABLE[code as usize] as i32).clamp(0, 88);
        code
    }

    fn decode_nibble(&mut self, nibble: u8) -> i16 {
        let step = STEP_TABLE[self.step_index as usize];
        let mut diff = step >> 3;
        if (nibble & 4) != 0 {
            diff += step;
        }
        if (nibble & 2) != 0 {
            diff += step >> 1;
        }
        if (nibble & 1) != 0 {
            diff += step >> 2;
        }
        if (nibble & 8) != 0 {
            self.predictor -= diff;
        } else {
            self.predictor += diff;
        }
        self.predictor = self.predictor.clamp(-32768, 32767);
        self.step_index = (self.step_index + INDEX_TABLE[(nibble & 0x0F) as usize] as i32).clamp(0, 88);
        self.predictor as i16
    }
}

/// Encode interleaved i16 PCM into stateless IMA-ADPCM block (initial predictor(s) + nibbles).
pub fn adpcm_encode(pcm: &[i16], channels: usize) -> Vec<u8> {
    if pcm.is_empty() {
        return Vec::new();
    }
    let frames = pcm.len() / channels;
    let mut out = Vec::with_capacity(channels * 4 + (frames * channels + 1) / 2);
    let mut states = [AdpcmState::default(); 2];
    for ch in 0..channels {
        states[ch].predictor = pcm[ch] as i32;
        out.extend_from_slice(&(pcm[ch]).to_le_bytes());
        out.push(0); // step index 0
        out.push(0); // reserved
    }
    let mut nibble_acc = 0u8;
    let mut high = false;
    for f in 1..frames {
        for ch in 0..channels {
            let n = states[ch].encode_nibble(pcm[f * channels + ch]);
            if !high {
                nibble_acc = n & 0x0F;
                high = true;
            } else {
                out.push(nibble_acc | ((n & 0x0F) << 4));
                high = false;
            }
        }
    }
    if high {
        out.push(nibble_acc);
    }
    out
}

pub fn adpcm_decode(data: &[u8], channels: usize, samples_per_ch: usize) -> Result<Vec<i16>> {
    ensure!(channels == 1 || channels == 2, "bad channels");
    if samples_per_ch == 0 {
        return Ok(Vec::new());
    }
    let hdr = channels * 4;
    ensure!(data.len() >= hdr, "truncated adpcm header");
    let mut states = [AdpcmState::default(); 2];
    let mut out = Vec::with_capacity(samples_per_ch * channels);
    for ch in 0..channels {
        let p = i16::from_le_bytes([data[ch * 4], data[ch * 4 + 1]]);
        let idx = data[ch * 4 + 2].min(88) as i32;
        states[ch] = AdpcmState { predictor: p as i32, step_index: idx };
        out.push(p);
    }
    let nibbles = &data[hdr..];
    let total_rem = (samples_per_ch - 1) * channels;
    ensure!(nibbles.len() * 2 >= total_rem, "truncated adpcm nibbles");
    let mut ni = 0usize;
    for _f in 1..samples_per_ch {
        for ch in 0..channels {
            let byte = nibbles[ni / 2];
            let nib = if ni % 2 == 0 { byte & 0x0F } else { (byte >> 4) & 0x0F };
            ni += 1;
            out.push(states[ch].decode_nibble(nib));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Windows WASAPI Capture (System Loopback + Mic) and Playback
// ---------------------------------------------------------------------------

fn pcm16_mix_format() -> windows::Win32::Media::Audio::WAVEFORMATEX {
    use windows::Win32::Media::Audio::{WAVEFORMATEX, WAVE_FORMAT_PCM};
    let channels = AUDIO_CHANNELS as u16;
    let bits = 16u16;
    let block_align = channels * (bits / 8);
    let avg_bytes = AUDIO_SAMPLE_RATE * block_align as u32;
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: channels,
        nSamplesPerSec: AUDIO_SAMPLE_RATE,
        nAvgBytesPerSec: avg_bytes,
        nBlockAlign: block_align,
        wBitsPerSample: bits,
        cbSize: 0,
    }
}

fn init_com_mta() {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_MULTITHREADED,
        );
    }
}

pub struct WasapiCapture {
    _client: windows::Win32::Media::Audio::IAudioClient,
    capture: windows::Win32::Media::Audio::IAudioCaptureClient,
    channels: usize,
}

impl WasapiCapture {
    /// Capture host system playback via WASAPI loopback (48kHz stereo PCM16).
    pub fn new_loopback() -> Result<Self> {
        Self::open(true)
    }

    /// Capture default microphone input (48kHz stereo PCM16).
    pub fn new_mic() -> Result<Self> {
        Self::open(false)
    }

    fn open(loopback: bool) -> Result<Self> {
        use windows::Win32::Media::Audio::{
            eCapture, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
            MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
        };
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

        init_com_mta();
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .context("MMDeviceEnumerator")?;
            let flow = if loopback { eRender } else { eCapture };
            let device = enumerator
                .GetDefaultAudioEndpoint(flow, eConsole)
                .context("GetDefaultAudioEndpoint")?;
            let client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .context("Activate IAudioClient")?;

            let fmt = pcm16_mix_format();
            let mut flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
            if loopback {
                flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
            }
            // 40ms ring buffer in 100ns units (400_000)
            client
                .Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 400_000, 0, &fmt, None)
                .context("IAudioClient::Initialize(capture)")?;

            let capture: IAudioCaptureClient = client.GetService().context("IAudioCaptureClient")?;
            client.Start().context("IAudioClient::Start(capture)")?;
            Ok(Self {
                _client: client,
                capture,
                channels: AUDIO_CHANNELS as usize,
            })
        }
    }

    /// Drain all currently available packets as interleaved PCM16 samples.
    pub fn read_pcm(&mut self) -> Result<Vec<i16>> {
        let mut out = Vec::new();
        unsafe {
            loop {
                let pkt_frames = self.capture.GetNextPacketSize().unwrap_or(0);
                if pkt_frames == 0 {
                    break;
                }
                let mut data_ptr: *mut u8 = std::ptr::null_mut();
                let mut num_frames = 0u32;
                let mut flags = 0u32;
                if self
                    .capture
                    .GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None)
                    .is_err()
                {
                    break;
                }
                let count = num_frames as usize * self.channels;
                // AUDCLNT_BUFFERFLAGS_SILENT = 0x2
                if (flags & 0x2) != 0 || data_ptr.is_null() {
                    out.resize(out.len() + count, 0i16);
                } else {
                    let src = std::slice::from_raw_parts(data_ptr as *const i16, count);
                    out.extend_from_slice(src);
                }
                let _ = self.capture.ReleaseBuffer(num_frames);
            }
        }
        Ok(out)
    }
}

pub struct WasapiPlayer {
    client: windows::Win32::Media::Audio::IAudioClient,
    render: windows::Win32::Media::Audio::IAudioRenderClient,
    buffer_frames: u32,
    channels: usize,
}

impl WasapiPlayer {
    /// Open default audio output endpoint at 48kHz stereo PCM16.
    pub fn new() -> Result<Self> {
        use windows::Win32::Media::Audio::{
            eConsole, eRender, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator,
            MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED,
        };
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

        init_com_mta();
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .context("MMDeviceEnumerator")?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .context("GetDefaultAudioEndpoint(render)")?;
            let client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .context("Activate IAudioClient(render)")?;
            let fmt = pcm16_mix_format();
            let flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
            // 100ms ring buffer (1_000_000 * 100ns)
            client
                .Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 1_000_000, 0, &fmt, None)
                .context("IAudioClient::Initialize(render)")?;
            let buffer_frames = client.GetBufferSize().context("GetBufferSize")?;
            let render: IAudioRenderClient = client.GetService().context("IAudioRenderClient")?;
            client.Start().context("IAudioClient::Start(render)")?;
            Ok(Self {
                client,
                render,
                buffer_frames,
                channels: AUDIO_CHANNELS as usize,
            })
        }
    }

    /// Push interleaved PCM16 samples into the render ring buffer (drops excess if congested).
    pub fn write_pcm(&mut self, pcm: &[i16]) -> Result<()> {
        if pcm.is_empty() {
            return Ok(());
        }
        let frames_in = (pcm.len() / self.channels) as u32;
        if frames_in == 0 {
            return Ok(());
        }
        unsafe {
            let padding = self.client.GetCurrentPadding().unwrap_or(0);
            let avail = self.buffer_frames.saturating_sub(padding);
            let to_write = frames_in.min(avail);
            if to_write == 0 {
                return Ok(());
            }
            let ptr = self.render.GetBuffer(to_write).context("Render::GetBuffer")?;
            let count = to_write as usize * self.channels;
            std::ptr::copy_nonoverlapping(pcm.as_ptr(), ptr as *mut i16, count);
            self.render.ReleaseBuffer(to_write, 0).context("Render::ReleaseBuffer")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adpcm_wire_roundtrip_silence_and_sine() {
        // 480 frames stereo sine @ 440Hz
        let mut pcm = Vec::with_capacity(480 * 2);
        for i in 0..480 {
            let v = ((i as f32 * 440.0 * std::f32::consts::TAU / 48000.0).sin() * 12000.0) as i16;
            pcm.push(v);
            pcm.push(-v);
        }
        let pkt = AudioPacket::encode_adpcm(48000, 2, &pcm).unwrap();
        // ~4:1 compression
        assert!(pkt.data.len() < pcm.len() * 2 / 3);
        let wire = pkt.to_wire();
        let back = AudioPacket::from_wire(&wire).unwrap();
        assert_eq!(back.sample_rate, 48000);
        assert_eq!(back.channels, 2);
        assert_eq!(back.samples_per_ch, 480);

        let decoded = back.decode_pcm().unwrap();
        assert_eq!(decoded.len(), pcm.len());
        // check RMS error stays low for smooth audio
        let mse: f64 = pcm
            .iter()
            .zip(decoded.iter())
            .map(|(&a, &b)| (a as f64 - b as f64).powi(2))
            .sum::<f64>()
            / pcm.len() as f64;
        let rmse = mse.sqrt();
        assert!(rmse < 600.0, "adpcm rmse too high: {rmse}");
    }

    #[test]
    fn wire_rejects_short_and_bad_codec() {
        assert!(AudioPacket::from_wire(&[0; 4]).is_err());
        let mut bad = AudioPacket::encode_adpcm(48000, 1, &[0, 0, 0, 0]).unwrap().to_wire();
        bad[5] = 99;
        assert!(AudioPacket::from_wire(&bad).is_err());
    }
}
