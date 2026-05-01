// Probe the WebSocket via the SAME URL the browser uses (nginx proxy at :3001).
// Capture all binary frames for 15 seconds, log sizes + intervals to detect
// whether the proxy is dropping frames after the initial LIMITED warmup frame.

const WebSocket = require('ws');
const URL = process.env.WS_URL || 'ws://192.168.2.132:3001/wss';
const TOKEN = 'Bearer dev-session-token';

const ws = new WebSocket(URL, { headers: { Authorization: TOKEN } });
const frames = [];
const start = Date.now();
let txt = 0;

ws.on('open', () => {
  console.log(`[+0ms] connected → ${URL}`);
  ws.send(JSON.stringify({ type: 'subscribe_position_updates', data: { binary: true, interval: 60 } }));
  console.log(`[+${Date.now() - start}ms] subscribe sent`);
});

ws.on('message', (m, isBin) => {
  const t = Date.now() - start;
  if (isBin) {
    frames.push({ t, len: m.length });
    if (frames.length <= 5 || frames.length % 30 === 0) {
      console.log(`[+${t}ms] bin#${frames.length} len=${m.length}`);
    }
  } else {
    txt++;
    if (txt <= 8) {
      const s = m.toString('utf8').slice(0, 120);
      console.log(`[+${t}ms] txt#${txt} ${s}`);
    }
  }
});

ws.on('error', (e) => console.error(`[+${Date.now() - start}ms] ERR ${e.message}`));
ws.on('close', (c) => console.log(`[+${Date.now() - start}ms] CLOSE ${c}`));

setTimeout(() => {
  const big = frames.filter(f => f.len > 100000).length;
  const small = frames.filter(f => f.len < 100000).length;
  console.log(`\n=== summary after 15s ===`);
  console.log(`  total bin frames=${frames.length}  small(<100KB)=${small}  full(>=100KB)=${big}`);
  if (frames.length > 0) {
    const last = frames[frames.length - 1];
    console.log(`  last frame: t=+${last.t}ms len=${last.len}`);
    const sizes = [...new Set(frames.map(f => f.len))].sort((a, b) => a - b);
    console.log(`  distinct sizes: ${sizes.slice(0, 10).join(', ')}${sizes.length > 10 ? ' …' : ''}`);
  }
  ws.close();
  process.exit(0);
}, 15000);
