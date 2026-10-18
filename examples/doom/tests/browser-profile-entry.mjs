// Bundled by browser-profile.mjs against the real Chittr decoder/player (read-only).
import createPlugin from '@extism/extism';
import { decodeCommandBuffer } from 'chittr/canvas/ops';
import { CommandPlayer } from 'chittr/canvas/apply';

window.runDoomProfile = async ({ wasmUrl, frames = 80 }) => {
  const canvas = document.createElement('canvas');
  canvas.width=320; canvas.height=200; document.body.append(canvas);
  const surface=canvas.getContext('2d');
  const player=new CommandPlayer({surface,families:new Set(),images:new Map(),
    imageData:(width,height,pixels)=>new ImageData(new Uint8ClampedArray(pixels),width,height)});
  const wasm = await WebAssembly.compileStreaming(fetch(wasmUrl));
  const wad = new Uint8Array(await (await fetch('/doom.wad')).arrayBuffer());
  const base64 = bytes => { let s=''; for (let i=0;i<bytes.length;i+=0x8000) s+=String.fromCharCode.apply(null,bytes.subarray(i,i+0x8000)); return btoa(s); };
  const calls=[];
  let callBytes=0,callSubmits=0,maxPacket=0;
  const plugin=await createPlugin(wasm,{useWasi:false,runInWorker:false,functions:{'extism:host/user':{
    set_fps(cp){return cp.store('{}');},request_frame(cp){return cp.store('{}');},
    file_read(cp,ptr){const {offset,length}=cp.read(ptr).json();const start=Number(offset);const chunk=wad.subarray(start,start+Number(length));return cp.store(JSON.stringify({bytes:base64(chunk),eof:start+chunk.length>=wad.length}));},
    file_close(cp){return cp.store('{}');},
    submit(cp,ptr){const bytes=cp.read(ptr).bytes();callBytes+=bytes.length;callSubmits++;
      maxPacket=Math.max(maxPacket,bytes.length);player.play(decodeCommandBuffer(bytes));},
  }}});
  async function call(name,input){
    callBytes=0;callSubmits=0;
    const start=performance.now();
    const output=await plugin.call(name,JSON.stringify(input));
    const ms=performance.now()-start;
    if(output?.json().error)throw new Error(output.text());
    calls.push({name,ms,bytes:callBytes,submits:callSubmits});
  }
  const event=input=>call('event',input);
  const key=(code,action)=>event({type:'key',code,action});
  const frame=(i,dt=1000/35)=>call('frame',{t:i*1000/35,dt});
  await call('start',{width:320,height:200,dpr:1,fullscreen:false,prefersReducedMotion:false,state:null,
    capabilities:['files.client','framebuffer.indexed8'],
    limits:{maxFileBytes:16777216,maxFileReadBytes:65536,maxFileHandles:4,maxFileBytesPerCanvas:33554432,maxPickers:8,maxFramebufferPixels:1048576,maxPaletteEntries:256}});
  await event({type:'file',name:'wad',file:{id:'f_profile',filename:'doom1.wad',mimeType:'application/octet-stream',size:wad.length}});
  await event({type:'pointer',action:'down'});await key('Space','down');await frame(0,0);await key('Space','up');
  const bootMs=calls.find(c=>c.name==='frame').ms;
  await key('ArrowUp','down');
  for(let i=1;i<=12;i++)await frame(i);
  const nativePixels=surface.getImageData(0,0,320,200).data;

  // Compare ABI-compatible scaling strategies with the actual browser decoder/player.
  // Group native horizontal runs by RGBA colour, then emit integer destination rectangles.
  const groups=new Map();
  for(let y=0;y<200;y++)for(let x=0;x<320;){
    const i=(y*320+x)*4,colour=((nativePixels[i]<<24)|(nativePixels[i+1]<<16)|(nativePixels[i+2]<<8)|255)>>>0;
    let end=x+1;
    while(end<320){const j=(y*320+end)*4;if(nativePixels[j]!==nativePixels[i]||nativePixels[j+1]!==nativePixels[i+1]||nativePixels[j+2]!==nativePixels[i+2])break;end++;}
    if(!groups.has(colour))groups.set(colour,[]);
    groups.get(colour).push([x,y,end-x,1]);x=end;
  }
  canvas.width=2048;canvas.height=2048;
  function commands(paths){
    const out=new Uint8Array(64*1024*22+4096),view=new DataView(out.buffer);let p=0;
    const byte=n=>out[p++]=n,u32=n=>{view.setUint32(p,n,true);p+=4;},f32=n=>{view.setFloat32(p,n,true);p+=4;};
    byte(0x30);u32(0x080808ff);byte(0x20);[0,0,2048,2048].forEach(f32);
    for(const [colour,runs]of groups){byte(0x30);u32(colour);if(paths)byte(0x10);
      for(const [x,y,w]of runs){const x0=Math.ceil(x*2048/320),x1=Math.ceil((x+w)*2048/320),y0=Math.ceil(y*1280/200),y1=Math.ceil((y+1)*1280/200);
        byte(paths?0x18:0x20);[x0,y0+384,x1-x0,y1-y0].forEach(f32);}
      if(paths){byte(0x1b);byte(0);}
    }
    return out.slice(0,p);
  }
  const strategies={};
  for(const paths of [false,true]){
    const bytes=commands(paths),samples=[];
    for(let i=0;i<15;i++){const start=performance.now();player.play(decodeCommandBuffer(bytes));samples.push(performance.now()-start);}
    strategies[paths?'paths':'rects']={bytes:bytes.length,maxMs:Math.max(...samples),meanMs:samples.reduce((a,b)=>a+b)/samples.length,runs:[...groups.values()].reduce((n,r)=>n+r.length,0)};
  }
  await event({type:'resize',width:2048,height:2048,fullscreen:true});
  const resize=calls.at(-1);
  const first=calls.length;
  for(let i=0;i<frames;i++){
    if(i===20)await key('ArrowRight','down');
    if(i===45){await key('ArrowRight','up');await key('Space','down');}
    if(i%5===0)await event({type:'pointer',action:'move',x:i,y:i});
    await frame(i+20,i%10===0?300:1000/35);
  }
  const measured=calls.slice(first),frameCalls=measured.filter(c=>c.name==='frame');
  const sorted=frameCalls.map(c=>c.ms).sort((a,b)=>a-b);
  const memory=(await plugin.getInstance()).exports.memory.buffer.byteLength;
  const summary={bootMs,resize,fullscreenFrames:frameCalls.length,maxFrameMs:sorted.at(-1),p95FrameMs:sorted[Math.floor(sorted.length*.95)],meanFrameMs:sorted.reduce((a,b)=>a+b)/sorted.length,
    maxEventMs:Math.max(...measured.filter(c=>c.name==='event').map(c=>c.ms)),maxFrameBytes:Math.max(...frameCalls.map(c=>c.bytes)),maxFrameSubmits:Math.max(...frameCalls.map(c=>c.submits)),maxPacket,memoryBytes:memory,strategies};
  await plugin.close();return summary;
};
