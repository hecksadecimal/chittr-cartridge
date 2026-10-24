// Optional integration test: npm's @extism/extism JS runtime, the same
// interface as the browser host. No Node dependency is needed to build or run
// the cartridge itself. EXTISM_JS_MODULE may point at an existing installation
// of @extism/extism.
//
// Schism Tracker is a Canvas Guest with no bundled game data. This test plays
// the host for the card's <lc-file> picker, feeds a tiny generated ProTracker
// module through file_read/file_close, then decodes the indexed (0x53) frame
// and checks that the original tracker plays it into audio.pcm.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
const require = createRequire(import.meta.url);
const imported = require(process.env.EXTISM_JS_MODULE || '@extism/extism');
const createPlugin = imported.default || imported;
const bytes = await readFile(new URL('../cartridge/canvas.wasm', import.meta.url));
assert(bytes.length < 4 * 1024 * 1024);
const module = await WebAssembly.compile(bytes);
assert(WebAssembly.Module.imports(module).every(i => i.module.startsWith('extism:host/')), 'no WASI imports');
assert.deepEqual(
  WebAssembly.Module.exports(module).filter(e => e.kind === 'function').map(e => e.name).sort(),
  ['event', 'frame', 'start'],
);

// A minimal 4-channel ProTracker module: one looped square-wave sample, one
// pattern, and a C-2 note on the first row. 2,236 bytes.
function makeMod() {
  const patternRows = 64, channels = 4;
  const sampleBytes = 128;
  const mod = Buffer.alloc(20 + 31 * 30 + 2 + 128 + 4 + patternRows * channels * 4 + sampleBytes);
  mod.write('Chittr smoke', 0, 'ascii');
  let at = 20;
  // Sample 1: 64 words, volume 48, loop over the whole sample.
  mod.write('square', at, 'ascii');
  mod.writeUInt16BE(sampleBytes / 2, at + 22);
  mod[at + 24] = 0; // finetune
  mod[at + 25] = 48; // volume
  mod.writeUInt16BE(0, at + 26); // loop start, words
  mod.writeUInt16BE(sampleBytes / 2, at + 28); // loop length, words
  at += 31 * 30;
  mod[at] = 1; // one order
  mod[at + 1] = 0; // restart order
  at += 2;
  mod[at] = 0; // order 0 -> pattern 0
  at += 128;
  mod.write('M.K.', at, 'ascii');
  at += 4;
  const period = 428; // C-2 in ProTracker's Amiga period table
  mod[at] = (period >> 8) & 0x0f;
  mod[at + 1] = period & 0xff;
  mod[at + 2] = 0x10; // sample 1, effect 0
  mod[at + 3] = 0;
  at += patternRows * channels * 4;
  for (let i = 0; i < sampleBytes; i++) mod[at + i] = i < sampleBytes / 2 ? 96 : -96;
  return mod;
}
const MOD = makeMod();
const HANDLE = 'f_smoke_module';

let state = { files: new Map([[HANDLE, MOD]]), audioWrites: 0, audioBytes: 0, audioNonZero: false,
  audioStops: 0, requests: 0, maxCallMs: 0, blits: 0, image: null, imageWidth: 0, imageHeight: 0,
  lastHash: '', hashes: new Set(), mode: 'indexed' };

function decodeSubmit(buffer) {
  const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  assert.equal(view.getUint8(0), 0x30, 'fillColor first');
  assert.equal(view.getUint8(5), 0x20, 'fillRect second');
  const op = view.getUint8(22);
  if (op === 0x53) {
    assert.equal(state.mode, 'indexed');
    let p = 23;
    const dx = view.getFloat32(p, true); p += 4;
    const dy = view.getFloat32(p, true); p += 4;
    const dw = view.getFloat32(p, true); p += 4;
    const dh = view.getFloat32(p, true); p += 4;
    const w = view.getUint32(p, true); p += 4;
    const h = view.getUint32(p, true); p += 4;
    const smoothing = view.getUint8(p); p += 1;
    const n = view.getUint32(p, true); p += 4;
    assert.equal(w, 640); assert.equal(h, 400);
    assert.equal(smoothing, 0, 'nearest-neighbour');
    assert.equal(n, 256);
    const palette = [];
    for (let i = 0; i < n; i++) { palette.push(view.getUint32(p, true)); p += 4; }
    const indices = buffer.subarray(p, p + w * h);
    assert.equal(p + w * h, buffer.length, 'the blit is the last op in the buffer');
    assert.equal(indices.length, 640 * 400);
    state.lastHash = createHash('sha256').update(buffer.subarray(23)).digest('hex');
    state.imageWidth = Math.floor(dw);
    state.imageHeight = Math.floor(dh);
    state.image = { dx, dy, dw, dh, indices };
    state.hashes.add(state.lastHash);
    state.blits++;
  } else if (op === 0x52) {
    assert.equal(state.mode, 'rgba');
    let p = 23;
    const dx = view.getFloat32(p, true); p += 4;
    const dy = view.getFloat32(p, true); p += 4;
    const dw = view.getFloat32(p, true); p += 4;
    const dh = view.getFloat32(p, true); p += 4;
    const w = view.getUint32(p, true); p += 4;
    const h = view.getUint32(p, true); p += 4;
    const smoothing = view.getUint8(p); p += 1;
    const length = view.getUint32(p, true); p += 4;
    assert.equal(w, 640); assert.equal(h, 400);
    assert.equal(smoothing, 0);
    assert.equal(length, 640 * 400 * 4);
    assert.equal(p + length, buffer.length);
    state.lastHash = createHash('sha256').update(buffer.subarray(23)).digest('hex');
    state.imageWidth = Math.floor(dw);
    state.imageHeight = Math.floor(dh);
    state.image = { dx, dy, dw, dh, indices: null };
    state.hashes.add(state.lastHash);
    state.blits++;
  }
}

function host() {
  return {
    'extism:host/user': {
      set_fps(cp, ptr) {
        const fps = cp.read(ptr).json().fps;
        assert.equal(Number(fps), 60);
        return cp.store('{}');
      },
      request_frame(cp) {
        state.requests++;
        return cp.store('{}');
      },
      file_read(cp, ptr) {
        const input = cp.read(ptr).json();
        const file = state.files.get(input.handle);
        assert(file, `unknown file handle ${input.handle}`);
        const start = Number(input.offset), length = Number(input.length);
        assert(length >= 1 && length <= 65536, 'a read is 1 to 64 KiB');
        const chunk = file.subarray(start, start + length);
        const eof = start + chunk.length >= file.length;
        return cp.store(JSON.stringify({ bytes: chunk.toString('base64'), eof }));
      },
      file_close(cp, ptr) {
        assert(state.files.delete(cp.read(ptr).json().handle), 'closing a live handle');
        return cp.store('{}');
      },
      audio_write(cp, ptr) {
        const input = cp.read(ptr).json();
        assert.equal(input.format, 's16le');
        assert.equal(Number(input.sampleRate), 48000);
        assert.equal(Number(input.channels), 2);
        const pcm = Buffer.from(input.samples, 'base64');
        assert(pcm.length > 0 && pcm.length <= 65536, 'one bounded PCM chunk');
        state.audioWrites++;
        state.audioBytes += pcm.length;
        if (pcm.some(byte => byte !== 0)) state.audioNonZero = true;
        // Pretend the host queue is empty so the next frame may mix again.
        return cp.store(JSON.stringify({ accepted: true, queuedMs: 0 }));
      },
      audio_stop(cp) {
        state.audioStops++;
        return cp.store('{}');
      },
      submit(cp, ptr) {
        decodeSubmit(cp.read(ptr).bytes());
        return cp.store('{}');
      },
    },
  };
}

async function open(mode, capabilities) {
  state = { ...state, files: new Map([[HANDLE, MOD]]), audioWrites: 0, audioBytes: 0,
    audioNonZero: false, audioStops: 0, requests: 0, maxCallMs: 0, blits: 0, image: null,
    imageWidth: 0, imageHeight: 0, lastHash: '', hashes: new Set(), mode };
  const plugin = await createPlugin(module, {
    runInWorker: false,
    useWasi: false,
    functions: host(),
  });
  async function call(name, input) {
    const start = performance.now();
    const output = await plugin.call(name, JSON.stringify(input));
    const elapsed = performance.now() - start;
    state.maxCallMs = Math.max(state.maxCallMs, elapsed);
    assert(elapsed < 500, `${name} exceeded 500ms: ${elapsed.toFixed(1)}ms`);
    const reply = output?.json();
    assert(!reply?.error, output?.text());
    return reply;
  }
  const key = async (code, action) => call('event', { type: 'key', code, key: code, action });
  const tap = async code => { await key(code, 'down'); await key(code, 'up'); };
  const startInput = {
    width: 640, height: 400, dpr: 1, fullscreen: false, prefersReducedMotion: false, state: null,
    capabilities,
    limits: { maxFileBytes: 16777216, maxFileReadBytes: 65536, maxFileHandles: 4,
      maxFileBytesPerCanvas: 41943040, maxPickers: 8, maxFramebufferPixels: 1048576,
      maxPaletteEntries: 256, maxRelativePointerDelta: 2000, maxPcmChunkBytes: 65536,
      pcmQueueMs: 2000 },
  };
  return { plugin, call, key, tap, startInput };
}

// Indexed path: boot the tracker, load a module through the picker, play it,
// and make sure real frames and real PCM reach the host.
{
  const { plugin, call, tap, startInput } = await open('indexed',
    ['files.client', 'audio.pcm', 'framebuffer.indexed8']);
  const started = performance.now();
  await call('start', startInput);
  assert(performance.now() - started < 5000, 'first boot stays responsive');
  assert.equal(state.requests, 0, 'start alone does not request frames');
  for (let i = 0; i < 4; i++) await call('frame', { t: i * 1000 / 60, dt: 1000 / 60 });
  assert(state.blits >= 1, 'engine presents an indexed frame');
  assert.equal(state.imageWidth, 640);
  assert.equal(state.imageHeight, 400);
  const emptyHash = state.lastHash;

  // The card's picker delivers only metadata; frames read the bytes in chunks.
  await call('event', { type: 'file', name: 'module',
    file: { id: HANDLE, filename: 'smoke.mod', mimeType: 'application/octet-stream', size: MOD.length } });
  let loadedHash = '';
  for (let i = 0; i < 20; i++) {
    await call('frame', { t: 100 + i * 1000 / 60, dt: 1000 / 60 });
    if (state.lastHash !== emptyHash) { loadedHash = state.lastHash; break; }
  }
  assert(loadedHash, 'loading a module changes the tracker screen');
  assert.equal(state.files.size, 0, 'the guest closes the picker handle after reading it');

  // The host reserves Escape, so the guest translates F10 to it. Both opening
  // and closing the menu must change the screen.
  const editorHash = state.lastHash;
  await tap('F10');
  for (let i = 0; i < 10 && state.lastHash === editorHash; i++) {
    await call('frame', { t: 500 + i * 1000 / 60, dt: 1000 / 60 });
  }
  assert.notEqual(state.lastHash, editorHash, 'F10 opens the tracker menu');
  const menuHash = state.lastHash;
  await tap('F10');
  for (let i = 0; i < 10 && state.lastHash === menuHash; i++) {
    await call('frame', { t: 700 + i * 1000 / 60, dt: 1000 / 60 });
  }
  assert.notEqual(state.lastHash, menuHash, 'F10 closes the tracker menu');

  // Ctrl-Q used to reach os_sysexit() through a prompt. In the browser it must
  // stay inside the guest and report that quitting is unavailable.
  await call('event', { type: 'key', code: 'ControlLeft', key: 'Control', action: 'down' });
  await call('event', { type: 'key', code: 'KeyQ', key: 'q', action: 'down' });
  await call('event', { type: 'key', code: 'KeyQ', key: 'q', action: 'up' });
  await call('event', { type: 'key', code: 'ControlLeft', key: 'Control', action: 'up' });
  await call('frame', { t: 900, dt: 1000 / 60 });

  await tap('F5');
  for (let i = 0; i < 90; i++) await call('frame', { t: 1000 + i * 1000 / 60, dt: 1000 / 60 });
  assert(state.audioWrites > 0, 'module playback reaches audio.pcm');
  assert(state.audioNonZero, 'the generated module produces non-silent PCM');
  assert(state.audioBytes > 4096, 'playback supplies more than one audio callback');
  assert(state.hashes.size >= 4, 'the tracker animates while playing');

  await tap('F8');
  const beforeBlur = state.requests;
  await call('event', { type: 'blur' });
  assert(state.audioStops > 0, 'blur stops PCM audio');
  await call('frame', { t: 5000, dt: 1000 / 60 });
  assert.equal(state.requests, beforeBlur, 'blur stops simulation and frame requests');
  await call('event', { type: 'pointer', action: 'down', x: 10, y: 10, width: 640, height: 400 });
  await call('event', { type: 'resize', width: 1280, height: 720, fullscreen: true });
  await call('frame', { t: 5016, dt: 1000 / 60 });
  assert.equal(state.imageWidth, 1152, '4:5 tracker keeps its aspect ratio');
  assert.equal(state.imageHeight, 720);
  assert.equal(state.image.dx, 64);
  assert.equal(state.image.dy, 0);
  await call('event', { type: 'input_reset' });
  await call('event', { type: 'resize', width: 640, height: 400, fullscreen: false });
  const instance = await plugin.getInstance();
  assert(instance.exports.memory.buffer.byteLength <= 64 * 1024 * 1024);
  console.log(JSON.stringify({ ok: true, path: 'indexed', wasmBytes: bytes.length,
    moduleBytes: MOD.length, memoryBytes: instance.exports.memory.buffer.byteLength,
    maxCallMs: +state.maxCallMs.toFixed(1), blits: state.blits, frames: state.hashes.size,
    audioWrites: state.audioWrites, audioBytes: state.audioBytes, audioStops: state.audioStops }));
  await plugin.close();
}

// RGBA fallback: the same engine frame must be presentable when the host does
// not grant indexed8 or a 256-entry palette.
{
  const { plugin, call, startInput } = await open('rgba', ['files.client', 'audio.pcm', 'framebuffer.rgba8']);
  await call('start', startInput);
  await call('frame', { t: 0, dt: 1000 / 60 });
  assert(state.blits >= 1, 'RGBA fallback presents a frame');
  assert.equal(state.imageWidth, 640);
  assert.equal(state.imageHeight, 400);
  console.log(JSON.stringify({ ok: true, path: 'rgba', blits: state.blits,
    maxCallMs: +state.maxCallMs.toFixed(1) }));
  await plugin.close();
}


