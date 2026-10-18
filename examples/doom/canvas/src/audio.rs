//! A small PCM mixer for DOOM's 8-bit DMX sound lumps. GPL-2.0.
//!
//! The C engine queues high-level sound events with their `sfxinfo` names. This
//! module decodes the matching `DS*` lump from the viewer's base or mod WADs, mixes up to a
//! bounded number of voices at 48,000 Hz stereo, and hands s16le chunks to the
//! host's `audio.pcm` queue. It is deliberately simple: no positional
//! attenuation, and no sample data lives in the cartridge.

use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use std::ffi::c_int;
use std::rc::Rc;
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    fn doom_music_active() -> c_int;
    fn doom_music_mix(out: *mut i16, frames: c_int);
    fn doom_music_pause(paused: c_int);
}

/// Pauses or resumes the C OPL music player. Playback is independent of the
/// SFX voice list; the host's audio queue is stopped separately when paused.
pub fn pause_music(paused: bool) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        doom_music_pause(if paused { 1 } else { 0 })
    };
    #[cfg(not(target_arch = "wasm32"))]
    let _ = paused;
}

const OUTPUT_RATE: u32 = 48_000;
const DEFAULT_SOURCE_RATE: u32 = 11_025;
const MAX_VOICES: usize = 16;
const MAX_FRAME_MS: f64 = 100.0;

struct Sound {
    rate: u32,
    samples: Vec<i16>,
}

struct Voice {
    sound: Rc<Sound>,
    position: f64,
    gain: f32,
}

impl Voice {
    fn step(&self) -> f64 {
        self.sound.rate as f64 / OUTPUT_RATE as f64
    }

    fn sample(&self) -> f32 {
        let position = self.position;
        let index = position as usize;
        let Some(first) = self.sound.samples.get(index).copied() else {
            return 0.0;
        };
        let second = self.sound.samples.get(index + 1).copied().unwrap_or(first);
        let fraction = position - index as f64;
        first as f32 + (second as f32 - first as f32) * fraction as f32
    }
}

#[allow(dead_code)]
pub struct Audio {
    wads: Vec<Arc<Vec<u8>>>,
    lumps: HashMap<String, (usize, usize, usize)>,
    cache: HashMap<String, Option<Rc<Sound>>>,
    voices: Vec<Voice>,
    first_frame: bool,
    queued_ms: u32,
}

#[allow(dead_code)]
impl Audio {
    pub fn new(wads: Vec<Arc<Vec<u8>>>) -> Self {
        let mut lumps = HashMap::new();
        for (index, wad) in wads.iter().enumerate() {
            for (name, (offset, size)) in scan_lumps(wad) {
                lumps.insert(name, (index, offset, size));
            }
        }
        Self {
            wads,
            lumps,
            cache: HashMap::new(),
            voices: Vec::new(),
            first_frame: true,
            queued_ms: 0,
        }
    }

    /// Queues one engine sound event. Unknown, malformed, or zero-volume sounds are ignored.
    pub fn push(&mut self, name: &str, volume: u8) {
        if volume == 0 {
            return;
        }
        let key = format!("DS{}", name.to_ascii_uppercase());
        if !self.cache.contains_key(&key) {
            let sound = self
                .lumps
                .get(&key)
                .and_then(|(wad, offset, size)| {
                    decode_dmx(&self.wads[*wad][*offset..*offset + *size])
                })
                .map(Rc::new);
            self.cache.insert(key.clone(), sound);
        }
        let Some(Some(sound)) = self.cache.get(&key).cloned() else {
            return;
        };
        self.voices.push(Voice {
            sound,
            position: 0.0,
            gain: (volume.min(15) as f32 / 15.0) * 0.12,
        });
        if self.voices.len() > MAX_VOICES {
            self.voices.remove(0);
        }
    }

    /// Mixes `dt_ms` of stereo s16le PCM at 48 kHz: the C engine's OPL music plus the
    /// bounded SFX voices. Returns `None` when neither has anything to play.
    pub fn mix(&mut self, dt_ms: f64) -> Option<Vec<u8>> {
        // The first frame after boot can carry the wall-clock time the viewer spent
        // before pressing Space. Do not queue that idle gap as audio latency.
        let millis = if dt_ms.is_finite() {
            if self.first_frame {
                self.first_frame = false;
                dt_ms.clamp(0.0, 50.0)
            } else {
                dt_ms.clamp(0.0, MAX_FRAME_MS)
            }
        } else {
            0.0
        };
        // If the host reports a deep PCM queue, produce less audio for a few frames
        // so the queue drains instead of turning into permanent SFX latency.
        let millis = if self.queued_ms > 500 {
            millis * 0.25
        } else if self.queued_ms > 120 {
            millis * 0.5
        } else {
            millis
        };
        let milliseconds = millis;
        let frames = (milliseconds * OUTPUT_RATE as f64 / 1000.0).round() as usize;
        if frames == 0 {
            return None;
        }
        #[cfg(target_arch = "wasm32")]
        let music = unsafe { doom_music_active() != 0 };
        #[cfg(not(target_arch = "wasm32"))]
        let music = false;
        if self.voices.is_empty() && !music {
            return None;
        }

        let mut samples = vec![0i16; frames * 2];
        for frame in 0..frames {
            let mut mixed = 0.0f32;
            for voice in &mut self.voices {
                mixed += voice.sample() * voice.gain;
                voice.position += voice.step();
            }
            let sample = mixed.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            samples[frame * 2] = sample;
            samples[frame * 2 + 1] = sample;
        }
        self.voices
            .retain(|voice| voice.position < voice.sound.samples.len() as f64);

        #[cfg(target_arch = "wasm32")]
        if music {
            // The C player adds the OPL mix in place and clamps each sample.
            unsafe { doom_music_mix(samples.as_mut_ptr(), frames as c_int) };
        }

        let mut pcm = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            pcm.extend_from_slice(&sample.to_le_bytes());
        }
        Some(pcm)
    }

    pub fn clear(&mut self) {
        self.voices.clear();
    }

    pub fn set_queued_ms(&mut self, queued_ms: u32) {
        self.queued_ms = queued_ms;
    }

    #[cfg(test)]
    fn has_voices(&self) -> bool {
        !self.voices.is_empty()
    }
}

fn scan_lumps(wad: &[u8]) -> HashMap<String, (usize, usize)> {
    let mut lumps = HashMap::new();
    let Some(count) = word(wad, 4) else {
        return lumps;
    };
    let Some(directory) = word(wad, 8) else {
        return lumps;
    };
    if count == 0 || count > 100_000 {
        return lumps;
    }
    for index in 0..count {
        let offset = directory + index * 16;
        let (Some(position), Some(size)) = (word(wad, offset), word(wad, offset + 4)) else {
            break;
        };
        let Some(name) = wad.get(offset + 8..offset + 16) else {
            break;
        };
        let length = name.iter().position(|byte| *byte == 0).unwrap_or(8);
        let Ok(name) = std::str::from_utf8(&name[..length]) else {
            continue;
        };
        if !name.starts_with("DS") {
            continue;
        }
        if position
            .checked_add(size)
            .is_some_and(|end| end <= wad.len())
        {
            lumps.insert(name.to_string(), (position, size));
        }
    }
    lumps
}

fn decode_dmx(bytes: &[u8]) -> Option<Sound> {
    if bytes.len() < 8 {
        return None;
    }
    let rate = u16::from_le_bytes([bytes[2], bytes[3]]) as u32;
    let count = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if count == 0 || count > 4_000_000 || count > bytes.len() - 8 {
        return None;
    }
    // DMX sound lumps in DOOM are unsigned 8-bit PCM, centered at 128.
    let samples = bytes[8..8 + count]
        .iter()
        .map(|byte| ((*byte as i16) - 128) << 8)
        .collect();
    Some(Sound {
        rate: if rate == 0 { DEFAULT_SOURCE_RATE } else { rate },
        samples,
    })
}

fn word(bytes: &[u8], offset: usize) -> Option<usize> {
    let data = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(data.try_into().ok()?) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_unsigned_dmx_samples() {
        let mut bytes = vec![3, 0, 0x11, 0x2b, 3, 0, 0, 0];
        bytes.extend_from_slice(&[0, 128, 255]);
        let sound = decode_dmx(&bytes).expect("decode");
        assert_eq!(sound.rate, 11_025);
        assert_eq!(sound.samples, vec![-32768, 0, 32512]);
    }

    #[test]
    fn rejects_truncated_or_unbounded_dmx() {
        assert!(decode_dmx(&[]).is_none());
        assert!(decode_dmx(&[3, 0, 0x11, 0x2b, 9, 0, 0, 0]).is_none());
        assert!(decode_dmx(&[3, 0, 0x11, 0x2b, 0, 0, 0, 0]).is_none());
    }

    #[test]
    fn mixes_and_retires_voices() {
        let mut audio = Audio {
            wads: Vec::new(),
            lumps: HashMap::new(),
            cache: HashMap::new(),
            voices: vec![Voice {
                sound: Rc::new(Sound {
                    rate: OUTPUT_RATE,
                    samples: vec![1000, -1000],
                }),
                position: 0.0,
                gain: 1.0,
            }],
            first_frame: false,
            queued_ms: 0,
        };
        let pcm = audio.mix(2.0 / OUTPUT_RATE as f64 * 1000.0).unwrap();
        assert_eq!(pcm.len(), 8);
        assert!(pcm.iter().any(|byte| *byte != 0));
        assert!(!audio.has_voices());
        assert!(audio.mix(16.0).is_none());
    }
}
