#!/usr/bin/env python3
import json
import re
import sys
from datetime import date
from pathlib import Path

import requests

BASE = Path(__file__).resolve().parents[1]
SOURCES = json.loads((BASE / 'sources.json').read_text())


def valid(line: str) -> bool:
    s = line.strip()
    if not s or s.startswith('!') or s.startswith('#') or s.startswith('['):
        return False
    if any(s.startswith(p) for p in ('$removeparam', '/')) and not s.startswith('@@||') and not s.startswith('||'):
        return False
    return True


def domain_key(rule: str):
    r = rule.strip()
    if r.startswith('@@'):
        r = r[2:]
    m = re.match(r'^\|\|([^\^/$|]+)', r)
    if m:
        return m.group(1)
    m = re.match(r'^(?:[0-9.]+\s+)?([A-Za-z0-9*._-]+\.[A-Za-z0-9._-]+)$', r)
    return m.group(1) if m else None


def load_rule_file(path: Path):
    rules = []
    for line in path.read_text().splitlines():
        s = line.strip()
        if valid(s):
            rules.append(s)
    return rules


allow = set(load_rule_file(BASE / 'custom-allowlist.txt'))
block = set(load_rule_file(BASE / 'custom-blocklist.txt'))

for src in SOURCES['whitelists']:
    txt = requests.get(src[1], timeout=120).text
    for line in txt.splitlines():
        if valid(line):
            s = line.strip()
            if not s.startswith('@@'):
                s = '@@' + s if s.startswith('||') else '@@' + s
            allow.add(s)

for src in SOURCES['filters']:
    txt = requests.get(src[1], timeout=180).text
    for line in txt.splitlines():
        if valid(line):
            s = line.strip()
            if s.startswith('@@'):
                allow.add(s)
            else:
                block.add(s)

allow_keys = {k for r in allow if (k := domain_key(r))}
merged_block = []
seen = set()
for s in sorted(block):
    k = domain_key(s)
    if k and k in allow_keys:
        continue
    if s not in seen:
        seen.add(s)
        merged_block.append(s)

merged_allow = []
seen = set()
for s in sorted(allow):
    if s not in seen:
        seen.add(s)
        merged_allow.append(s)

(BASE / 'blocklist.txt').write_text(
    '! YT merged AdGuard blocklist\n'
    f'! Generated: {date.today().isoformat()}\n'
    '! Sources: enabled upstream blocklists + upstream allow exceptions + custom rules\n\n'
    + '\n'.join(merged_block) + '\n'
)
(BASE / 'allowlist.txt').write_text(
    '! YT merged AdGuard allowlist\n'
    f'! Generated: {date.today().isoformat()}\n'
    '! Sources: enabled upstream allowlists + custom allow rules + upstream allow exceptions\n\n'
    + '\n'.join(merged_allow) + '\n'
)

overlap = []
block_keys = {k for r in merged_block if (k := domain_key(r))}
for r in merged_allow:
    k = domain_key(r)
    if k and k in block_keys:
        overlap.append((r, k))
if overlap:
    print('ERROR: overlap remains', overlap[:20], file=sys.stderr)
    sys.exit(2)

print(f'OK block={len(merged_block)} allow={len(merged_allow)}')
