// Optional integration test: npm's @extism/extism JS runtime, the same interface as the browser host.
// No Node dependency is needed to build or run the cartridge itself.
// EXTISM_JS_MODULE may point at an existing installation of @extism/extism.
//
// The engine build loads its WADs at runtime: this test plays the host for the
// <lc-file> pickers and file_read/file_close, then decodes the single
// blitIndexedScaled (0x53) op the guest submits each frame.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { performance } from 'node:perf_hooks';
const require = createRequire(import.meta.url);
const imported = require(process.env.EXTISM_JS_MODULE || '@extism/extism');
const createPlugin = imported.default || imported;
const bytes = await readFile(new URL('../cartridge/canvas.wasm', import.meta.url));
assert(bytes.length < 4 * 1024 * 1024);
const module = await WebAssembly.compile(bytes);
assert(WebAssembly.Module.imports(module).every(i => i.module.startsWith('extism:host/')), 'no WASI imports');
assert.deepEqual(WebAssembly.Module.exports(module).filter(e => e.kind === 'function').map(e => e.name).sort(), ['event','frame','start']);

// The viewer picks these WADs through the card's <lc-file target="client"> pickers.
const WAD = new Uint8Array(await readFile(process.env.DOOM_WAD || new URL('../cartridge/assets/doom1.wad', import.meta.url)));
const HANDLE = 'f_smoke_wad';
const MOD = process.env.DOOM_MOD_WAD ? new Uint8Array(await readFile(process.env.DOOM_MOD_WAD)) : null;
const BASE_PICKER = process.env.DOOM_PICKER || "wad";
const MOD_HANDLE = 'f_smoke_mod';
const WADS = new Map([[HANDLE, WAD], ...(MOD ? [[MOD_HANDLE, MOD]] : [])]);

let image, imageWidth, imageHeight, requests = 0, maxCallMs = 0, maxReadMs = 0;
let surfaceWidth = 320, surfaceHeight = 200, blitsThisCall = 0, largestSubmit = 0;
let audioWrites = 0, audioBytes = 0, audioStops = 0, audioNonZero = false, maxPcmBytes = 0;
const plugin = await createPlugin(module, {
  runInWorker: false, useWasi: false,
  functions: { 'extism:host/user': {
    set_fps(cp, ptr) { assert([15,35].includes(cp.read(ptr).json().fps)); return cp.store('{}'); },
    request_frame(cp) { requests++; return cp.store('{}'); },
    file_read(cp, ptr) {
      const started = performance.now();
      const input = cp.read(ptr).json();
      const wad = WADS.get(input.handle);
      assert(wad, `unknown file handle ${input.handle}`);
      const start = Number(input.offset), length = Number(input.length);
      assert(length >= 1 && length <= 65536, 'a read is 1 to 64 KiB');
      const chunk = wad.subarray(start, start + length);
      const eof = start + chunk.length >= wad.length;
      const out = cp.store(JSON.stringify({ bytes: Buffer.from(chunk).toString('base64'), eof }));
      maxReadMs = Math.max(maxReadMs, performance.now() - started);
      return out;
    },
    file_close(cp, ptr) { WADS.delete(cp.read(ptr).json().handle); return cp.store('{}'); },
    audio_write(cp, ptr) {
      const input = cp.read(ptr).json();
      assert.equal(input.format, 's16le');
      assert.equal(Number(input.sampleRate), 48000);
      assert.equal(Number(input.channels), 2);
      const pcm = Buffer.from(input.samples, 'base64');
      assert(pcm.length > 0 && pcm.length <= 65536, 'one bounded PCM chunk');
      audioWrites++;
      audioBytes += pcm.length;
      maxPcmBytes = Math.max(maxPcmBytes, pcm.length);
      if (pcm.some(byte => byte !== 0)) audioNonZero = true;
      return cp.store(JSON.stringify({ accepted: true, queuedMs: 0 }));
    },
    audio_stop(cp) { audioStops++; return cp.store('{}'); },
    submit(cp, ptr) {
      const buffer = cp.read(ptr).bytes();
      assert(buffer.length <= 4*1024*1024);
      largestSubmit = Math.max(largestSubmit, buffer.length);
      const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
      // Every frame starts with fillColor (0x30) + fillRect (0x20); an engine
      // frame then carries one blitIndexedScaled (0x53) covering the whole fit.
      assert.equal(view.getUint8(0), 0x30, 'fillColor first');
      assert.equal(view.getUint8(5), 0x20, 'fillRect second');
      if (view.getUint8(22) !== 0x53) return; // idle/message frame: no framebuffer
      let p = 23;
      const dx = view.getFloat32(p, true); p += 4;
      const dy = view.getFloat32(p, true); p += 4;
      const dw = view.getFloat32(p, true); p += 4;
      const dh = view.getFloat32(p, true); p += 4;
      const w = view.getUint32(p, true); p += 4;
      const h = view.getUint32(p, true); p += 4;
      const smoothing = view.getUint8(p); p += 1;
      const n = view.getUint32(p, true); p += 4;
      assert.equal(w, 320); assert.equal(h, 200);
      assert.equal(smoothing, 0, 'nearest-neighbour');
      assert(n >= 1 && n <= 256);
      const palette = [];
      for (let i = 0; i < n; i++) { palette.push(view.getUint32(p, true)); p += 4; }
      const indices = buffer.subarray(p, p + w*h);
      assert.equal(p + w*h, buffer.length, 'the op is the last thing in the buffer');
      const scale = Math.min(surfaceWidth/320, surfaceHeight/200);
      const fitWidth = Math.max(1, Math.floor(320*scale));
      const fitHeight = Math.max(1, Math.floor(200*scale));
      assert.equal(dw, fitWidth); assert.equal(dh, fitHeight);
      assert(Math.abs(dx - (surfaceWidth-fitWidth)/2) < 1e-3);
      assert(Math.abs(dy - (surfaceHeight-fitHeight)/2) < 1e-3);
      imageWidth = fitWidth; imageHeight = fitHeight;
      image = new Uint8Array(imageWidth*imageHeight*4);
      for (let y = 0; y < fitHeight; y++) {
        for (let x = 0; x < fitWidth; x++) {
          const source = Math.floor(y*200/fitHeight)*320 + Math.floor(x*320/fitWidth);
          const entry = palette[indices[source]] ?? 0;
          const o = (y*fitWidth + x)*4;
          image[o] = (entry>>>24)&0xff; image[o+1] = (entry>>>16)&0xff;
          image[o+2] = (entry>>>8)&0xff; image[o+3] = entry&0xff;
        }
      }
      blitsThisCall++;
    },
  } },
});
async function call(name, input) {
  if(name === 'start' || (name === 'event' && input.type === 'resize')) {
    surfaceWidth=Math.max(1,Math.min(2048,input.width));
    surfaceHeight=Math.max(1,Math.min(2048,input.height));
  }
  blitsThisCall=0;
  const start = performance.now();
  const output = await plugin.call(name, JSON.stringify(input));
  const elapsed = performance.now()-start;
  maxCallMs = Math.max(maxCallMs, elapsed);
  assert(elapsed < 250, `${name} exceeded 250ms: ${elapsed.toFixed(1)}ms`);
  assert(!output?.json().error, output?.text());
  if(blitsThisCall) assert.equal(blitsThisCall,1,'exactly one indexed frame per draw');
}
const key = async (code, action) => call('event', {type:'key',code,action});
const hash = () => createHash('sha256').update(image).digest('hex');
await call('start', {width:320,height:200,dpr:1,fullscreen:false,prefersReducedMotion:false,state:null,
  capabilities:['files.client','framebuffer.indexed8','input.relative-pointer','audio.pcm'],
  limits:{maxFileBytes:33554432,maxFileReadBytes:65536,maxFileHandles:4,maxFileBytesPerCanvas:41943040,maxPickers:8,maxFramebufferPixels:1048576,maxPaletteEntries:256,maxRelativePointerDelta:2000,maxPcmChunkBytes:65536,pcmQueueMs:2000}});
assert.equal(requests,0, 'idle until deliberate interaction');
await key('Space','down'); assert.equal(requests,0,'Space before click must not start');
// The picker delivers the WAD metadata; the guest reads it in bounded chunks.
await call('event',{type:'file',name:BASE_PICKER,file:{id:HANDLE,filename:BASE_PICKER === 'wad' ? 'base.wad' : `${BASE_PICKER}.wad`,mimeType:'application/octet-stream',size:WAD.length}});
if (MOD) {
  // The guest reads one WAD at a time; let the base finish before adding the mod.
  for (let i = 0; i < 220; i++) await call('frame',{t:i*30,dt:30});
  await call('event',{type:'file',name:'mod',file:{id:MOD_HANDLE,filename:'mod.wad',mimeType:'application/octet-stream',size:MOD.length}});
}
await call('event',{type:'pointer',action:'down',x:10,y:10,width:320,height:200});
await key('Space','down');
// The file event only registers the handle; frames read the WAD in bounded chunks.
for (let i = 0; i < 400 && !image; i++) await call('frame',{t:i*30,dt:30});
assert(image, 'engine emitted an actual indexed framebuffer');
assert(audioWrites > 0, 'level music reached audio.pcm');
assert(audioNonZero, 'music PCM is not silent');
const first = hash();
await key('ArrowUp','down');
for(let i=1;i<=35;i++) await call('frame',{t:i*1000/35,dt:1000/35});
await key('ArrowUp','up');
assert.notEqual(hash(),first,'movement changes the engine framebuffer');
const beforeMouse = hash();
await call('event',{type:'pointer',relative:true,movementX:300,movementY:0,buttons:0});
await call('frame',{t:1600,dt:1000/35});
assert.notEqual(hash(),beforeMouse,'relative mouse changes the view');
// Movement semantics are checked at the native engine boundary (player x/y);
// a rendered frame can also change from ordinary view/weapon animation.
await call('event',{type:'pointer',relative:true,movementX:0,movementY:300,buttons:0});
await call('frame',{t:1630,dt:1000/35});
await call('event',{type:'pointer',relative:true,movementX:0,movementY:0,buttons:8|16});
await call('frame',{t:1660,dt:1000/35});
await call('event',{type:'input_reset'});
await key('Space','down');
for(let i=36;i<=53;i++) await call('frame',{t:i*1000/35,dt:1000/35});
await key('Space','up');
await key('KeyF','down'); await call('frame',{t:1600,dt:1000/35}); await key('KeyF','up');
await call('event',{type:'blur'});
assert(audioStops > 0, 'blur stops PCM audio');
const pausedRequests=requests;
await call('frame',{t:2000,dt:1000});
assert.equal(requests,pausedRequests,'blur stops simulation/frame requests');
await call('event',{type:'pointer',action:'down'});
await call('event',{type:'resize',width:2048,height:2048,fullscreen:true});
assert.equal(imageWidth,2048); assert.equal(imageHeight,1280);
await call('frame',{t:2030,dt:30});
await call('event',{type:'resize',width:1920,height:1080,fullscreen:true});
assert.equal(imageWidth,1728); assert.equal(imageHeight,1080);
await call('event',{type:'resize',width:1000,height:700,fullscreen:true});
assert.equal(imageWidth,1000); assert.equal(imageHeight,625);
const instance=await plugin.getInstance();
assert(instance.exports.memory.buffer.byteLength <= 64*1024*1024);
if(process.env.DOOM_SCREENSHOT) {
  // Portable RGB PPM: no image-writing dependency required.
  const rgb=Buffer.alloc(imageWidth*imageHeight*3);
  for(let i=0;i<image.length/4;i++) rgb.set(image.subarray(i*4,i*4+3),i*3);
  await writeFile(process.env.DOOM_SCREENSHOT,Buffer.concat([Buffer.from(`P6\n${imageWidth} ${imageHeight}\n255\n`),rgb]));
}
await call('event',{type:'resize',width:320,height:200,fullscreen:false});
// Open the real engine menu, select its sixth item (Quit), and confirm. This
// exercises the returning wasm quit path and avoids reading a stopped NULL framebuffer.
async function tap(code) { await key(code,'down'); await key(code,'up'); }
await tap('F10'); await call('frame',{t:2060,dt:1000/35});
await tap('KeyY'); await call('frame',{t:2090,dt:1000/35});
const stoppedRequests=requests;
await call('frame',{t:2180,dt:1000/35});
assert.equal(requests,stoppedRequests,'quit stops without trapping or scheduling more frames');
console.log(JSON.stringify({ok:true,wasmBytes:bytes.length,wadBytes:WAD.length,modBytes:MOD?.length??0,memoryBytes:instance.exports.memory.buffer.byteLength,maxCallMs:+maxCallMs.toFixed(1),maxReadMs:+maxReadMs.toFixed(1),largestSubmit,audioWrites,audioBytes,maxPcmBytes,audioStops,frame:hash()}));
await plugin.close();
