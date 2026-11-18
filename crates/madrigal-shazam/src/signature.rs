use std::f64::consts::PI;

pub const SAMPLE_RATE_HZ: u32 = 16000;
pub const SIGNATURE_PREFIX: &str = "data:audio/vnd.shazam.sig;base64,";

const FFT_SIZE: usize = 2048;
const HOP: usize = 128;
const BINS: usize = 1025;
const RING: usize = 256;
const DEFAULT_MAX_TIME_SECONDS: f64 = 3.1;
const DEFAULT_MAX_PEAKS: usize = 255;

pub struct Signature {
    binary: Vec<u8>,
    pub uri: String,
    pub sample_duration_ms: u64,
}

impl Signature {
    pub fn as_bytes(&self) -> &[u8] {
        &self.binary
    }
}

struct Peak {
    fft_pass: i32,
    magnitude: i32,
    corrected_bin: i32,
}

pub struct SignatureGenerator {
    max_time_seconds: f64,
    max_peaks: usize,
    pending: Vec<i32>,
    processed: usize,
    ring: [i32; FFT_SIZE],
    ring_pos: usize,
    real: Box<[f64]>,
    imag: Box<[f64]>,
    fft: Fft,
    window: Box<[f64]>,
    fft_outputs: Box<[[f64; BINS]]>,
    fft_pos: usize,
    fft_written: i32,
    spread_outputs: Box<[[f64; BINS]]>,
    spread_pos: usize,
    spread_written: i32,
    number_samples: i32,
    band_peaks: [Vec<Peak>; 4],
}

impl SignatureGenerator {
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_MAX_TIME_SECONDS, DEFAULT_MAX_PEAKS)
    }

    pub fn with_limits(max_time_seconds: f64, max_peaks: usize) -> Self {
        let mut window = vec![0.0; FFT_SIZE].into_boxed_slice();
        for (i, slot) in window.iter_mut().enumerate() {
            let j = (i + 1) as f64;
            *slot = 0.5 - 0.5 * (2.0 * PI * j / 2049.0).cos();
        }
        Self {
            max_time_seconds,
            max_peaks,
            pending: Vec::new(),
            processed: 0,
            ring: [0; FFT_SIZE],
            ring_pos: 0,
            real: vec![0.0; FFT_SIZE].into_boxed_slice(),
            imag: vec![0.0; FFT_SIZE].into_boxed_slice(),
            fft: Fft::new(),
            window,
            fft_outputs: vec![[0.0; BINS]; RING].into_boxed_slice(),
            fft_pos: 0,
            fft_written: 0,
            spread_outputs: vec![[0.0; BINS]; RING].into_boxed_slice(),
            spread_pos: 0,
            spread_written: 0,
            number_samples: 0,
            band_peaks: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
        }
    }

    pub fn feed_pcm16_mono(&mut self, samples: &[i16]) {
        self.pending.extend(samples.iter().map(|s| *s as i32));
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.processed = 0;
        self.ring = [0; FFT_SIZE];
        self.ring_pos = 0;
        self.fft_pos = 0;
        self.fft_written = 0;
        self.spread_pos = 0;
        self.spread_written = 0;
        self.number_samples = 0;
        for peaks in &mut self.band_peaks {
            peaks.clear();
        }
    }

    pub fn peak_count(&self) -> usize {
        self.band_peaks.iter().map(Vec::len).sum()
    }

    pub fn next_signature(&mut self) -> Option<Signature> {
        if self.pending.len() - self.processed < HOP {
            return None;
        }
        while self.pending.len() - self.processed >= HOP
            && (f64::from(self.number_samples) / f64::from(SAMPLE_RATE_HZ)
                < self.max_time_seconds
                || self.peak_count() < self.max_peaks)
        {
            let end = self.processed + HOP;
            self.process_input(self.processed, end);
            self.processed = end;
        }
        if self.peak_count() == 0 {
            return None;
        }
        let binary = encode_binary(SAMPLE_RATE_HZ, self.number_samples, &self.band_peaks);
        let signature = Signature {
            uri: format!("{SIGNATURE_PREFIX}{}", base64_encode(&binary)),
            sample_duration_ms: (i64::from(self.number_samples) * 1000)
                .div_euclid(i64::from(SAMPLE_RATE_HZ)) as u64,
            binary,
        };
        self.reset();
        Some(signature)
    }

    fn process_input(&mut self, start: usize, end: usize) {
        self.number_samples += (end - start) as i32;
        let mut pos = start;
        while pos < end {
            self.do_fft(pos);
            self.do_peak_spreading();
            if self.spread_written >= 46 {
                self.do_peak_recognition();
            }
            pos += HOP;
        }
    }

    fn do_fft(&mut self, start: usize) {
        for i in 0..HOP {
            self.ring[self.ring_pos] = self.pending[start + i];
            self.ring_pos += 1;
            if self.ring_pos == self.ring.len() {
                self.ring_pos = 0;
            }
        }
        let mut p = self.ring_pos;
        for idx in 0..FFT_SIZE {
            if p == FFT_SIZE {
                p = 0;
            }
            self.real[idx] = f64::from(self.ring[p]) * self.window[idx];
            self.imag[idx] = 0.0;
            p += 1;
        }
        self.fft.transform(&mut self.real, &mut self.imag);
        let out = &mut self.fft_outputs[self.fft_pos];
        for (k, slot) in out.iter_mut().enumerate().take(BINS) {
            let re = self.real[k];
            let im = self.imag[k];
            let v = (re * re + im * im) / 131072.0;
            *slot = if v <= 1e-10 { 1e-10 } else { v };
        }
        self.fft_pos += 1;
        if self.fft_pos == RING {
            self.fft_pos = 0;
        }
        self.fft_written += 1;
    }

    fn do_peak_spreading(&mut self) {
        let last = floor_mod(self.fft_pos as i32 - 1, RING as i32) as usize;
        let mut origin_spread = [0.0; BINS];
        for i in 0..=1021 {
            origin_spread[i] = self.fft_outputs[last][i]
                .max(self.fft_outputs[last][i + 1])
                .max(self.fft_outputs[last][i + 2]);
        }
        origin_spread[1022] = self.fft_outputs[last][1022];
        origin_spread[1023] = self.fft_outputs[last][1023];
        origin_spread[1024] = self.fft_outputs[last][1024];

        let i1 = floor_mod(self.spread_pos as i32 - 1, RING as i32) as usize;
        let i2 = floor_mod(self.spread_pos as i32 - 3, RING as i32) as usize;
        let i3 = floor_mod(self.spread_pos as i32 - 6, RING as i32) as usize;
        for bin in 0..BINS {
            let m1 = origin_spread[bin].max(self.spread_outputs[i1][bin]);
            self.spread_outputs[i1][bin] = m1;
            let m2 = m1.max(self.spread_outputs[i2][bin]);
            self.spread_outputs[i2][bin] = m2;
            let m3 = m2.max(self.spread_outputs[i3][bin]);
            self.spread_outputs[i3][bin] = m3;
        }
        self.spread_outputs[self.spread_pos] = origin_spread;
        self.spread_pos += 1;
        if self.spread_pos == RING {
            self.spread_pos = 0;
        }
        self.spread_written += 1;
    }

    fn do_peak_recognition(&mut self) {
        let fft_idx = floor_mod(self.fft_pos as i32 - 46, RING as i32) as usize;
        let spread_idx = floor_mod(self.spread_pos as i32 - 49, RING as i32) as usize;
        let mut bin = 10;
        while bin <= 1014 {
            let energy = self.fft_outputs[fft_idx][bin];
            if energy >= 1.0 / 64.0 && energy >= self.spread_outputs[spread_idx][bin - 1] {
                let mut peak = 0.0f64;
                for offset in [-10i32, -7, -4, -3, 1, 2, 5, 8] {
                    peak = peak.max(
                        self.spread_outputs[spread_idx]
                            [(bin as i32 + offset) as usize],
                    );
                }
                if energy > peak {
                    let mut time_peak = peak;
                    for offset in [
                        -53i32, -45, 165, 172, 179, 186, 193, 200, 214, 221, 228, 235, 242, 249,
                    ] {
                        let idx =
                            floor_mod(self.spread_pos as i32 + offset, RING as i32) as usize;
                        time_peak =
                            time_peak.max(self.spread_outputs[idx][bin - 1]);
                    }
                    if energy > time_peak {
                        self.register_peak(fft_idx, bin, energy);
                    }
                }
            }
            bin += 1;
        }
    }

    fn register_peak(&mut self, fft_idx: usize, bin: usize, energy: f64) {
        let magnitude = (energy.max(1.0 / 64.0).ln() * 1477.3 + 6144.0) as i32;
        let before =
            (self.fft_outputs[fft_idx][bin - 1].max(1.0 / 64.0).ln() * 1477.3 + 6144.0) as i32;
        let after =
            (self.fft_outputs[fft_idx][bin + 1].max(1.0 / 64.0).ln() * 1477.3 + 6144.0) as i32;
        let variation = f64::from(magnitude) * 2.0 - f64::from(before) - f64::from(after);
        if variation <= 0.0 {
            return;
        }
        let corrected =
            bin as f64 * 64.0 + f64::from(after - before) * 32.0 / variation;
        let freq_hz = corrected * (f64::from(SAMPLE_RATE_HZ) / 2.0 / 1024.0 / 64.0);
        let Some(band) = band_of(freq_hz) else {
            return;
        };
        self.band_peaks[band].push(Peak {
            fft_pass: self.spread_written - 46,
            magnitude,
            corrected_bin: corrected as i32,
        });
    }
}

impl Default for SignatureGenerator {
    fn default() -> Self {
        Self::new()
    }
}

fn band_of(freq_hz: f64) -> Option<usize> {
    if (250.0..520.0).contains(&freq_hz) {
        Some(0)
    } else if freq_hz > 520.0 && freq_hz <= 1450.0 {
        Some(1)
    } else if freq_hz > 1450.0 && freq_hz <= 3500.0 {
        Some(2)
    } else if freq_hz > 3500.0 && freq_hz <= 5500.0 {
        Some(3)
    } else {
        None
    }
}

fn floor_mod(value: i32, modulus: i32) -> i32 {
    value.rem_euclid(modulus)
}

fn encode_binary(sample_rate_hz: u32, number_samples: i32, bands: &[Vec<Peak>; 4]) -> Vec<u8> {
    let mut contents = Vec::new();
    for (band, peaks) in bands.iter().enumerate() {
        let mut body = Vec::new();
        let mut last_pass = 0;
        for peak in peaks {
            let mut delta = peak.fft_pass - last_pass;
            if delta >= 255 {
                body.push(0xFF);
                body.extend_from_slice(&peak.fft_pass.to_le_bytes());
                delta = 0;
            }
            body.push(delta.clamp(0, 254) as u8);
            body.extend_from_slice(&(peak.magnitude as i16).to_le_bytes());
            body.extend_from_slice(&(peak.corrected_bin as i16).to_le_bytes());
            last_pass = peak.fft_pass;
        }
        contents.extend_from_slice(&0x6003_0040u32.wrapping_add(band as u32).to_le_bytes());
        contents.extend_from_slice(&(body.len() as u32).to_le_bytes());
        contents.extend_from_slice(&body);
        let padding = body.len().rem_euclid(4);
        if padding != 0 {
            contents.extend(std::iter::repeat_n(0, 4 - padding));
        }
    }
    let size_minus_header = contents.len() as u32 + 8;
    let mut full = Vec::with_capacity(48 + 8 + contents.len());
    full.extend_from_slice(&0xCAFE_2580u32.to_le_bytes());
    full.extend_from_slice(&0u32.to_le_bytes());
    full.extend_from_slice(&size_minus_header.to_le_bytes());
    full.extend_from_slice(&0x9411_9C00u32.to_le_bytes());
    full.extend_from_slice(&[0u8; 12]);
    full.extend_from_slice(&(3u32 << 27).to_le_bytes());
    full.extend_from_slice(&[0u8; 8]);
    let total = number_samples as f64 + f64::from(sample_rate_hz) * 0.24;
    full.extend_from_slice(&(total as u32).to_le_bytes());
    full.extend_from_slice(&((15u32 << 19) + 0x40000u32).to_le_bytes());
    full.extend_from_slice(&0x4000_0000u32.to_le_bytes());
    full.extend_from_slice(&size_minus_header.to_le_bytes());
    full.extend_from_slice(&contents);
    let crc = crc32(&full[8..]);
    full[4..8].copy_from_slice(&crc.to_le_bytes());
    full
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            if crc & 1 == 1 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFF_FFFF
}

pub fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let mut word = 0u32;
        for byte in chunk {
            word = (word << 8) | u32::from(*byte);
        }
        word <<= 8 * (3 - chunk.len());
        out.push(ALPHABET[((word >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((word >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((word >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(word & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

struct Fft {
    cos_table: Vec<f64>,
    sin_table: Vec<f64>,
    bit_reversal: Vec<usize>,
}

impl Fft {
    fn new() -> Self {
        let n = FFT_SIZE;
        let mut cos_table = vec![0.0; n / 2];
        let mut sin_table = vec![0.0; n / 2];
        for (i, (cos, sin)) in cos_table.iter_mut().zip(sin_table.iter_mut()).enumerate() {
            *cos = (2.0 * PI * i as f64 / n as f64).cos();
            *sin = (2.0 * PI * i as f64 / n as f64).sin();
        }
        let bits = (FFT_SIZE as u32).trailing_zeros();
        let mut bit_reversal = vec![0; n];
        for (i, slot) in bit_reversal.iter_mut().enumerate() {
            *slot = reverse_bits(i, bits);
        }
        Self {
            cos_table,
            sin_table,
            bit_reversal,
        }
    }

    fn transform(&self, real: &mut [f64], imag: &mut [f64]) {
        for (i, slot) in self.bit_reversal.iter().enumerate() {
            let j = *slot;
            if j > i {
                real.swap(i, j);
                imag.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= FFT_SIZE {
            let half = len / 2;
            let step = FFT_SIZE / len;
            let mut i = 0;
            while i < FFT_SIZE {
                let mut j = 0;
                let mut k = 0;
                while j < half {
                    let cos = self.cos_table[k];
                    let sin = self.sin_table[k];
                    let a = i + j;
                    let b = a + half;
                    let tpre = real[b] * cos + imag[b] * sin;
                    let tpim = -real[b] * sin + imag[b] * cos;
                    real[b] = real[a] - tpre;
                    imag[b] = imag[a] - tpim;
                    real[a] += tpre;
                    imag[a] += tpim;
                    j += 1;
                    k += step;
                }
                i += len;
            }
            len <<= 1;
        }
    }
}

fn reverse_bits(mut value: usize, count: u32) -> usize {
    let mut out = 0;
    for _ in 0..count {
        out = (out << 1) | (value & 1);
        value >>= 1;
    }
    out
}
