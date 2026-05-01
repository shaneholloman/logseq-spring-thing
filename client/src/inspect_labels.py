#!/usr/bin/env python3
"""Sample labels per node type to understand label-population gaps."""
import json, sys, collections, urllib.request

req = urllib.request.Request(
    'http://localhost:4000/api/graph/data',
    headers={'Authorization': 'Bearer dev-session-token'},
)
with urllib.request.urlopen(req) as resp:
    r = json.loads(resp.read())

nodes = r.get('data', {}).get('nodes', [])
print(f'total nodes: {len(nodes)}')

for t in ['page', 'owl_class', 'kg_stub']:
    g = [n for n in nodes if n.get('type') == t]
    empty = sum(1 for n in g if not n.get('label'))
    has_meta_id = sum(1 for n in g if n.get('metadataId'))
    print(f'\n=== {t} count={len(g)} empty_label={empty} has_metadataId={has_meta_id} ===')
    samples = [n for n in g if n.get('label')][:3]
    print('  WITH labels:')
    for n in samples:
        meta_keys = list((n.get('metadata') or {}).keys())[:6]
        print(f"    id={n['id']} label={n['label']!r} mId={n.get('metadataId','')!r} mkeys={meta_keys}")
    samples_empty = [n for n in g if not n.get('label')][:3]
    print('  WITHOUT labels:')
    for n in samples_empty:
        meta_keys = list((n.get('metadata') or {}).keys())[:6]
        print(f"    id={n['id']} label='' mId={n.get('metadataId','')!r} mkeys={meta_keys} meta={n.get('metadata')}")
