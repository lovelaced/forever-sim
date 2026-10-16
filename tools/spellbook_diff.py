#!/usr/bin/env python3
"""List what changed in Forever's spell and talent data between two builds.

  node tools/forever_db.mjs --build <old> --out old.json
  node tools/forever_db.mjs --build <new> --out new.json
  python3 tools/spellbook_diff.py old.json new.json [class|race ...] [--json out.json]

Spells are grouped by class (or race, for racials) and named with their
level, so ranks read as "Frostbolt (20)". For each spell: added, removed, or
every field that changed, old -> new. Talents: added, removed, moved (points
needed), ranks, prerequisites and per-rank values.
"""
import json, sys

argv = [a for a in sys.argv[1:]]
out_json = None
if '--json' in argv:
    i = argv.index('--json')
    out_json = argv[i + 1]
    del argv[i:i + 2]
old_path, new_path, *only = argv
old = json.load(open(old_path))
new = json.load(open(new_path))


def group(s):
    return s.get('class') or s.get('race') or 'triggered'


def label(s):
    lv = s.get('level')
    return f"{s.get('name')} ({lv})" if lv else s.get('name')


def flat(v, prefix=''):
    """Spell record as {dotted field: value}; effects keyed by their index."""
    out = {}
    if isinstance(v, dict):
        for k, x in v.items():
            out.update(flat(x, f'{prefix}{k}.' if prefix or k else k))
    elif isinstance(v, list) and v and all(isinstance(e, dict) and 'i' in e for e in v):
        for e in v:
            out.update(flat({k: x for k, x in e.items() if k != 'i'}, f"{prefix}{e['i']}."))
    else:
        out[prefix.rstrip('.')] = v
    return out


def spell_changes():
    res = {}
    o, n = old['spells'], new['spells']
    for sid in sorted(set(o) | set(n), key=int):
        a, b = o.get(sid), n.get(sid)
        s = b or a
        g = group(s)
        if only and g not in only:
            continue
        if a is None:
            res.setdefault(g, []).append((label(b), sid, 'added', None))
        elif b is None:
            res.setdefault(g, []).append((label(a), sid, 'removed', None))
        else:
            fa, fb = flat({k: v for k, v in a.items() if k not in ('id',)}), flat({k: v for k, v in b.items() if k not in ('id',)})
            diffs = [(k, fa.get(k), fb.get(k)) for k in sorted(set(fa) | set(fb)) if fa.get(k) != fb.get(k)]
            if diffs:
                res.setdefault(g, []).append((label(b), sid, 'changed', diffs))
    return res


def talent_changes():
    res = {}
    ot, nt = old.get('talents', {}), new.get('talents', {})
    for cls in sorted(set(ot) | set(nt)):
        if only and cls not in only:
            continue
        a = {t['node']: t for t in ot.get(cls, [])}
        b = {t['node']: t for t in nt.get(cls, [])}
        rows = []
        for node in sorted(set(a) | set(b)):
            x, y = a.get(node), b.get(node)
            if x is None:
                rows.append((y['name'], 'added', [('points', None, y['points']), ('ranks', None, y['ranks']), ('requires', None, y['requires']), ('values', None, y['values'])]))
            elif y is None:
                rows.append((x['name'], 'removed', None))
            else:
                d = [(k, x.get(k), y.get(k)) for k in ('name', 'spell', 'ranks', 'points', 'requires', 'linked', 'values') if x.get(k) != y.get(k)]
                if d:
                    rows.append((y['name'], 'changed', d))
        if rows:
            res[cls] = rows
    return res


def show(v):
    return json.dumps(v, separators=(',', ':')) if isinstance(v, (list, dict)) else str(v)


sp, tl = spell_changes(), talent_changes()
print(f"# {old.get('build')} -> {new.get('build')}\n")
for g in sorted(set(sp) | set(tl)):
    print(f'## {g}')
    for name, kind, d in tl.get(g, []):
        if kind == 'changed':
            print(f'  talent {name}: ' + '; '.join(f'{k} {show(a)} -> {show(b)}' for k, a, b in d))
        else:
            print(f'  talent {name}: {kind}' + (': ' + '; '.join(f'{k} {show(b)}' for k, _, b in d) if d else ''))
    for name, sid, kind, d in sp.get(g, []):
        if kind == 'changed':
            print(f'  {name} [{sid}]: ' + '; '.join(f'{k} {show(a)} -> {show(b)}' for k, a, b in d))
        else:
            print(f'  {name} [{sid}]: {kind}')
    print()
if out_json:
    json.dump({'spells': {g: [dict(name=n, id=i, kind=k, diffs=d) for n, i, k, d in v] for g, v in sp.items()},
               'talents': {g: [dict(name=n, kind=k, diffs=d) for n, k, d in v] for g, v in tl.items()}},
              open(out_json, 'w'), indent=1)
