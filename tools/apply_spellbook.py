#!/usr/bin/env python3
"""Fill spell ranks in data/base/classes/*.toml from Forever's spell data.

For every spell rank that `fsim audit` matched to a game spell, this sets:
  id                    the game's spell id for the rank
  lo, hi                direct damage or healing when the rank is learned,
  lo_per_level, ...     and how it grows with the caster's level,
  max_level             up to the rank's top level
  mana                  the rank's mana cost (when the rank lists one)
  cast                  the rank's cast time (when the rank lists one)
  coef                  the game's spell power coefficient (and the spell's
                        direct damage or healing then reads `coef = "r.coef"`)
Only fields a spell already uses the standard way are touched: lo/hi when its
first direct damage or healing effect reads `min = "r.lo", max = "r.hi"`.
Everything else is left for review by hand; run `fsim audit` afterwards.

  python3 tools/apply_spellbook.py [class ...]
"""
import json, re, subprocess, sys, tomllib, tempfile, os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLASSES = sys.argv[1:] or ['warrior', 'paladin', 'hunter', 'rogue', 'priest', 'shaman', 'mage', 'warlock', 'druid']

book = json.load(open(os.path.join(ROOT, 'data/reference/spellbook.json')))['spells']
with tempfile.NamedTemporaryFile(suffix='.json', delete=False) as t:
    out = t.name
subprocess.run([os.path.join(ROOT, 'target/release/fsim'), '--data', os.path.join(ROOT, 'data'), 'audit', '--json', out],
               cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
rows = json.load(open(out))
os.unlink(out)
match = {}
for r in rows:
    if r['game_id'] is not None:
        match[(r['class'], r['spell'], r['level'])] = r['game_id']


def first_fx(g, kinds):
    return next((e for e in g.get('effects', []) if e.get('type') in kinds), None)


def fmt(v):
    if isinstance(v, float):
        v = round(v, 4)
        if v == int(v):
            return str(int(v))
        return repr(v)
    return str(v)


def rank_line(fields):
    return '  { ' + ', '.join(f'{k} = {fmt(v)}' for k, v in fields.items()) + ' },'


def direct_effect(sp):
    """The spell's first direct (not periodic, not weapon) damage or heal effect."""
    for e in sp.get('effects', []):
        if e.get('type') in ('damage', 'heal') and not e.get('periodic') and 'weapon' not in e:
            return e
    return None


changed = {}
for cls in CLASSES:
    path = os.path.join(ROOT, f'data/base/classes/{cls}.toml')
    text = open(path).read()
    data = tomllib.loads(text)
    n = 0
    for sid, sp in data.get('spells', {}).items():
        ranks = sp.get('ranks')
        if not isinstance(ranks, list) or not ranks:
            continue
        eff = direct_effect(sp)
        uses_lohi = eff is not None and eff.get('min') == 'r.lo' and eff.get('max') == 'r.hi'
        kind = ('school_damage',) if eff is not None and eff.get('type') == 'damage' else ('heal',)
        coef_src = eff.get('coef') if eff is not None else None
        coef_simple = isinstance(coef_src, (int, float)) or (isinstance(coef_src, str) and re.fullmatch(r'[\d.\s/*+()-]*(r\.(cast|coef))?[\d.\s/*+()-]*', coef_src or '') is not None)
        new_ranks = []
        any_coef = False
        for r in ranks:
            gid = match.get((cls, sid, r['level']))
            g = book.get(str(gid)) if gid else None
            f = dict(r)
            if not g:
                new_ranks.append(f)
                continue
            out_f = {'level': r['level'], 'id': gid}
            fx = first_fx(g, kind)
            lvl = g.get('level') or r['level']
            top = g.get('maxLevel') or 0
            recompute_lohi = uses_lohi and fx is not None and 'base' in fx
            recompute_coef = coef_simple and fx is not None and bool(fx.get('coef'))
            for k, v in r.items():
                # fields this tool recomputes are rebuilt below; any others the
                # data already has (growth or top level on other fields) stay
                if k in ('level', 'id'):
                    continue
                if k in ('lo_per_level', 'hi_per_level', 'max_level') and recompute_lohi:
                    continue
                if k == 'coef' and recompute_coef:
                    continue
                if k in ('lo', 'hi') and uses_lohi and fx and 'base' in fx:
                    var = fx.get('variance', 0.0)
                    side = -1 if k == 'lo' else 1
                    out_f[k] = round(fx['base'] * (1 + side * var / 2))
                    per = fx.get('perLevel', 0.0) * (1 + side * var / 2)
                    if per and top > lvl:
                        out_f[f'{k}_per_level'] = round(per, 3)
                    continue
                if k == 'mana':
                    cost = next((c for c in g.get('cost', []) if c.get('power') == 'mana' and c.get('amount')), None)
                    out_f[k] = cost['amount'] if cost else v
                    continue
                if k == 'cast' and 'cast' in g:
                    out_f[k] = max(g['cast'], 0)
                    continue
                out_f[k] = v
            if uses_lohi and fx and fx.get('perLevel') and top > lvl:
                out_f['max_level'] = top
            if coef_simple and fx and fx.get('coef'):
                out_f['coef'] = fx['coef']
                any_coef = True
            new_ranks.append(out_f)
        # rewrite the ranks block of this spell
        head = re.search(rf'^\[spells\.{re.escape(sid)}\]\s*$', text, re.M)
        if not head:
            continue
        nxt = re.search(r'^\[', text[head.end():], re.M)
        end = head.end() + (nxt.start() if nxt else len(text) - head.end())
        block = text[head.end():end]
        m = re.search(r'^ranks = \[\n(.*?)^\]\n', block, re.M | re.S)
        if not m:
            continue
        lines = '\n'.join(rank_line(f) for f in new_ranks)
        # the effect's coefficient now reads the rank's (only after the ranks)
        rest = block[m.end():]
        if any_coef and isinstance(coef_src, str) and coef_src != 'r.coef':
            rest = rest.replace(f'coef = "{coef_src}"', 'coef = "r.coef"', 1)
        elif any_coef and isinstance(coef_src, (int, float)):
            rest = re.sub(rf'coef = {re.escape(str(coef_src))}(?![\d.])', 'coef = "r.coef"', rest, count=1)
        nb = block[:m.start()] + 'ranks = [\n' + lines + '\n]\n' + rest
        if nb != block:
            text = text[:head.end()] + nb + text[end:]
            n += 1
    open(path, 'w').write(text)
    changed[cls] = n
print('spells updated:', changed)
