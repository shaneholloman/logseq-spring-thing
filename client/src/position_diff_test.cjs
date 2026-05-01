// Settings → Physics live-update regression check.
//
// Procedure:
//   1. Open WS, capture one full binary frame (positions BEFORE).
//   2. Decode 5 KG + 5 ontology samples.
//   3. POST a physics-setting change (repel_k +50%) via /api/settings.
//   4. Wait 4 s for the GPU reheat-on-param-change pulse to propagate.
//   5. Capture another full frame (positions AFTER).
//   6. Diff sample positions, report.
//
// Wire layout per ADR-037 / binary_protocol.rs:130-153 :
//   header 9 bytes: u8 version + u64 LE broadcast_sequence
//   then per node 48 bytes:
//     u32 LE id-with-flags  (bit 31 agent, bit 30 knowledge, bit 29 private,
//                            bits 26-28 ontology subtype, bits 0-25 = id)
//     3xf32 LE position
//     3xf32 LE velocity
//     f32 sssp_distance, i32 sssp_parent, u32 cluster, f32 anomaly, u32 community

const WebSocket = require('ws');
const http = require('http');

const HOST = process.env.HOST || 'localhost';
const PORT = parseInt(process.env.PORT || '4000', 10);
const URL  = `ws://${HOST}:${PORT}/wss`;
const TOKEN = 'Bearer dev-session-token';

const FLAG_AGENT     = 0x80000000;
const FLAG_KNOWLEDGE = 0x40000000;
const FLAG_PRIVATE   = 0x20000000;
const MASK_ONTOLOGY  = 0x1C000000;
const MASK_ID        = 0x03FFFFFF;

function decodeFrame(buf) {
  if (buf.length < 9) return [];
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const version = view.getUint8(0);
  if (version !== 5) return [];
  const nodes = [];
  let off = 9;
  while (off + 48 <= buf.length) {
    const idFlagged = view.getUint32(off, true);
    const id        = idFlagged & MASK_ID;
    const isAgent   = !!(idFlagged & FLAG_AGENT);
    const isKnow    = !!(idFlagged & FLAG_KNOWLEDGE);
    const isPriv    = !!(idFlagged & FLAG_PRIVATE);
    const onto      = (idFlagged & MASK_ONTOLOGY) >>> 26;
    const x = view.getFloat32(off + 4, true);
    const y = view.getFloat32(off + 8, true);
    const z = view.getFloat32(off + 12, true);
    const vx = view.getFloat32(off + 16, true);
    const vy = view.getFloat32(off + 20, true);
    const vz = view.getFloat32(off + 24, true);
    nodes.push({ id, idFlagged, isAgent, isKnow, isPriv, onto, x, y, z, vx, vy, vz });
    off += 48;
  }
  return nodes;
}

function captureOneFrame(timeoutMs = 4000) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(URL, { headers: { Authorization: TOKEN } });
    let done = false;
    const t = setTimeout(() => { if (!done) { done = true; ws.close(); reject(new Error('frame timeout')); } }, timeoutMs);
    ws.on('open', () => {
      ws.send(JSON.stringify({ type: 'subscribe_position_updates', data: { binary: true, interval: 60 } }));
    });
    ws.on('message', (m, isBin) => {
      if (!isBin || done) return;
      // Skip the small initial 9601-byte LIMITED frame; wait for the full ~1.2 MB frame.
      if (m.length < 100000) return;
      done = true;
      clearTimeout(t);
      const nodes = decodeFrame(m);
      ws.close();
      resolve(nodes);
    });
    ws.on('error', (e) => { if (!done) { done = true; clearTimeout(t); reject(e); } });
  });
}

// Updates one setting via PUT /api/settings/path { path, value }.
// This is the live client path (autoSaveManager → settingsApi.updateSettingsByPaths).
function putSettingPath(path, value) {
  return new Promise((resolve, reject) => {
    const body = JSON.stringify({ path, value });
    const req = http.request({
      host: HOST, port: PORT, path: '/api/settings/path',
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(body),
        'Authorization': TOKEN,
      },
    }, (res) => {
      let data = '';
      res.on('data', (c) => data += c);
      res.on('end', () => resolve({ status: res.statusCode, body: data.slice(0, 400) }));
    });
    req.on('error', reject);
    req.write(body);
    req.end();
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  console.log('1) Capturing BEFORE frame…');
  const before = await captureOneFrame();
  console.log(`   captured ${before.length} nodes`);

  // Pick 5 KG + 5 ontology samples. Knowledge flag is bit 30 + bits 26-28 == 0;
  // ontology layer in this build is identified by ontology subtype bits != 0
  // (knowledge_node_flag is also set on KG nodes). Per memory, OntologyClass is
  // emitted with bits 26-28 != 0; KGNode 'page' nodes carry knowledge flag.
  // Diagnostic: histogram of flag patterns + raw bits of first 3.
  const flagHist = new Map();
  for (const n of before.slice(0, 5000)) {
    const key = `agent=${+n.isAgent} know=${+n.isKnow} priv=${+n.isPriv} onto=${n.onto}`;
    flagHist.set(key, (flagHist.get(key) || 0) + 1);
  }
  console.log('   flag histogram (first 5000):');
  [...flagHist.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8).forEach(([k, v]) =>
    console.log(`     ${v.toString().padStart(6)}  ${k}`));
  console.log('   first 3 raw idFlagged bits:',
    before.slice(0, 3).map(n => '0x' + n.idFlagged.toString(16).padStart(8, '0')).join(' '));

  const kg   = before.filter(n => n.isKnow && n.onto === 0 && !n.isAgent).slice(0, 5);
  let ontoFallback = before.filter(n => n.onto !== 0).slice(0, 5);
  if (ontoFallback.length === 0) ontoFallback = before.filter(n => n.isPriv).slice(0, 5);
  // Final: positional split if both flag classes are absent.
  if (kg.length === 0 && ontoFallback.length === 0) {
    kg.push(...before.slice(0, 5));
    ontoFallback.push(...before.slice(Math.floor(before.length / 2), Math.floor(before.length / 2) + 5));
    console.log('   (positional split — flag bits absent)');
  }
  console.log(`   sampled ${kg.length} KG + ${ontoFallback.length} ontology`);
  const samples = [...kg, ...ontoFallback];
  if (samples.length === 0) { console.error('no samples!'); process.exit(2); }
  for (const n of samples) {
    console.log(`   ${labelFor(n)}  id=${n.id}  pos=(${n.x.toFixed(2)}, ${n.y.toFixed(2)}, ${n.z.toFixed(2)})  |v|=${Math.hypot(n.vx,n.vy,n.vz).toFixed(4)}`);
  }

  console.log('\n2) PUT settings change: visualisation.graphs.logseq.physics.repel_k → 8000');
  const r = await putSettingPath('visualisation.graphs.logseq.physics.repel_k', 8000.0);
  console.log(`   PUT ${r.status}: ${r.body.slice(0, 200)}`);

  console.log('\n3) Waiting 4 s for reheat to propagate…');
  await sleep(4000);

  console.log('\n4) Capturing AFTER frame…');
  const after = await captureOneFrame();
  console.log(`   captured ${after.length} nodes`);

  // Index after by id for fast lookup.
  const idx = new Map();
  for (const n of after) idx.set(n.id, n);

  console.log('\n5) Position deltas for samples:');
  console.log('     LAYER    ID         ΔX         ΔY         ΔZ      |Δ|       |v_after|');
  let movedKg = 0, movedOnto = 0;
  let totalAbsDeltaKg = 0, totalAbsDeltaOnto = 0;
  for (const n of samples) {
    const a = idx.get(n.id);
    if (!a) { console.log(`   ${labelFor(n)} id=${n.id}  MISSING in after frame`); continue; }
    const dx = a.x - n.x, dy = a.y - n.y, dz = a.z - n.z;
    const mag = Math.hypot(dx, dy, dz);
    const vmag = Math.hypot(a.vx, a.vy, a.vz);
    const isKg = kg.find((k) => k.id === n.id);
    if (isKg) { totalAbsDeltaKg += mag; if (mag > 0.01) movedKg++; }
    else { totalAbsDeltaOnto += mag; if (mag > 0.01) movedOnto++; }
    console.log(`     ${labelFor(n).padEnd(8)}  ${String(n.id).padStart(8)}  ${dx.toFixed(3).padStart(9)}  ${dy.toFixed(3).padStart(9)}  ${dz.toFixed(3).padStart(9)}  ${mag.toFixed(3).padStart(7)}  ${vmag.toFixed(4)}`);
  }

  console.log('\n=== SUMMARY ===');
  console.log(`  KG samples moved (>0.01u): ${movedKg}/${kg.length}   total|Δ|=${totalAbsDeltaKg.toFixed(3)}`);
  console.log(`  Ontology samples moved   : ${movedOnto}/${ontoFallback.length}   total|Δ|=${totalAbsDeltaOnto.toFixed(3)}`);

  // Overall stats across whole graph for context.
  let totalAfter = 0, movedTotal = 0, maxDelta = 0;
  for (const n of before) {
    const a = idx.get(n.id);
    if (!a) continue;
    const m = Math.hypot(a.x - n.x, a.y - n.y, a.z - n.z);
    totalAfter++;
    if (m > 0.01) movedTotal++;
    if (m > maxDelta) maxDelta = m;
  }
  console.log(`  Whole graph: ${movedTotal}/${totalAfter} moved (>0.01u), max |Δ|=${maxDelta.toFixed(3)}`);

  console.log('\n6) Restoring repel_k → 5000');
  const r2 = await putSettingPath('visualisation.graphs.logseq.physics.repel_k', 5000.0);
  console.log(`   PUT ${r2.status}`);
  process.exit(0);
}

function labelFor(n) {
  if (n.isAgent) return 'AGENT';
  if (n.onto !== 0) return 'ONTO';
  if (n.isPriv) return 'STUB';
  if (n.isKnow) return 'KG';
  return '?';
}

main().catch((e) => { console.error('FATAL', e); process.exit(1); });
