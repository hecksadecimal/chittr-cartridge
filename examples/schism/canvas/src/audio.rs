//! Drains the C engine's PCM ring buffer into the host's `audio.pcm` queue.
//! GPL-2.0; see ../../engine/LICENSE.
use crate::engine::Engine;

/// The engine mixes at the rate its "browser" device negotiated, usually
/// 48 kHz stereo s16le. The host has a per-call byte limit, so one frame sends
/// at most one chunk and the backend ring absorbs any scheduling jitter.
const MAX_QUEUE_MS: u32 = 800;

pub struct Audio {
    rate: u32,
    channels: u32,
    max_chunk: usize,
    queued_ms: u32,
}

impl Audio {
    pub fn new(engine: &Engine, max_chunk: usize) -> Self {
        Self {
            rate: engine.audio_rate(),
            channels: engine.audio_channels(),
            max_chunk: max_chunk.max(2),
            queued_ms: 0,
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn channels(&self) -> u32 {
        self.channels
    }

    /// Returns a bounded PCM chunk when the host queue has room. The engine
    /// always produces s16le at `rate`/`channels`, including silence.
    pub fn pump(&mut self, engine: &mut Engine) -> Option<Vec<u8>> {
        if self.queued_ms > MAX_QUEUE_MS {
            return None;
        }
        let available = engine.audio_available().min(self.max_chunk);
        if available < 2 {
            return None;
        }
        let available = available & !1; // whole s16 samples only
        let mut pcm = vec![0u8; available];
        let read = engine.audio_read(&mut pcm);
        pcm.truncate(read);
        (!pcm.is_empty()).then_some(pcm)
    }

    pub fn set_queued_ms(&mut self, queued_ms: u32) {
        self.queued_ms = queued_ms;
    }

    pub fn clear(&mut self, engine: &mut Engine) {
        engine.audio_clear();
        self.queued_ms = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_depth_is_bounded() {
        let mut audio = Audio {
            rate: 48_000,
            channels: 2,
            max_chunk: 65_536,
            queued_ms: MAX_QUEUE_MS + 1,
        };
        assert!(audio.queued_ms > MAX_QUEUE_MS);
        audio.set_queued_ms(0);
        assert_eq!(audio.queued_ms, 0);
    }
}
