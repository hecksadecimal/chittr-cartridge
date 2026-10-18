// Optional real-browser profiling. Read-only reuse of Chittr's actual command decoder/player.
// CHITTR_SOURCE=/path/to/Chittr PLAYWRIGHT_MODULE=/path/to/playwright node tests/browser-profile.mjs
// DOOM_WASM overrides the wasm under test; CPU_RATE=4 simulates a slower client.
import {createRequire} from 'node:module';
import {readFile} from 'node:fs/promises';
import {createServer} from 'node:http';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
const source=process.env.CHITTR_SOURCE;
if(!source)throw new Error('Set CHITTR_SOURCE to an existing Chittr checkout (read-only)');
const esbuild=createRequire(resolve(source,'package.json'))('esbuild');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const root=dirname(fileURLToPath(import.meta.url));
const wasm=await readFile(process.env.DOOM_WASM||resolve(root,'../cartridge/canvas.wasm'));
const wad=await readFile(process.env.DOOM_WAD||resolve(root,'../cartridge/assets/doom1.wad'));
const bundle=await esbuild.build({entryPoints:[resolve(root,'browser-profile-entry.mjs')],bundle:true,write:false,format:'iife',platform:'browser',
  alias:{'@extism/extism':resolve(source,'node_modules/@extism/extism/dist/browser/mod.js'),
    'chittr/canvas/ops':resolve(source,'lib/live-chitts/canvas/ops.ts'),'chittr/canvas/apply':resolve(source,'lib/live-chitts/canvas/apply.ts')}});
const server=createServer((req,res)=>{
  if(req.url==='/guest.wasm'){res.setHeader('Content-Type','application/wasm');res.end(wasm);}
  else if(req.url==='/doom.wad'){res.setHeader('Content-Type','application/octet-stream');res.end(wad);}
  else if(req.url==='/profile.js'){res.setHeader('Content-Type','text/javascript');res.end(bundle.outputFiles[0].contents);}
  else{res.setHeader('Content-Type','text/html');res.end('<!doctype html><html><body><script src="/profile.js"></script></body></html>');}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
let browser;
try{
  browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:{})});
  const page=await browser.newPage();
  const cdp=await page.context().newCDPSession(page);
  await cdp.send('Emulation.setCPUThrottlingRate',{rate:Number(process.env.CPU_RATE||1)});
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  const summary=await page.evaluate(async frames=>window.runDoomProfile({wasmUrl:'/guest.wasm',frames}),Number(process.env.FRAMES||80));
  console.log(JSON.stringify({cpuRate:Number(process.env.CPU_RATE||1),...summary},null,2));
  if(process.env.ASSERT_FRAME_MS && summary.maxFrameMs>=Number(process.env.ASSERT_FRAME_MS))throw new Error('Fullscreen frame budget exceeded');
}finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
