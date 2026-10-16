#!/usr/bin/env python3
"""Look up spells in data/reference/spellbook.json (Forever's spell data).

  python3 tools/spell.py "Lightning Bolt"          # every spell of that name
  python3 tools/spell.py "Lightning Bolt" shaman   # only that class's
  python3 tools/spell.py 15208 10438               # by id
  python3 tools/spell.py --grep "totem" shaman     # names containing text
  python3 tools/spell.py --text "Fire Power" mage   # with the tooltip text
  python3 tools/spell.py --grep "" troll            # a race's racials

--text reads the tooltip from the cached tables (.db2/<build>/Spell.csv,
fetched by tools/forever_db.mjs); the spellbook itself holds only numbers.
"""
import csv, json, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
data = json.load(open(os.path.join(ROOT, 'data/reference/spellbook.json')))
book = data['spells']
args = sys.argv[1:]
grep = '--grep' in args
text = '--text' in args
args = [a for a in args if a not in ('--grep', '--text')]
if not args:
    sys.exit(__doc__)
tips = {}
if text:
    path = os.path.join(ROOT, '.db2', data.get('build', ''), 'Spell.csv')
    if os.path.exists(path):
        csv.field_size_limit(10**9)
        for r in csv.DictReader(open(path, encoding='utf-8')):
            tips[r['ID']] = r
    else:
        print(f'no tooltips cached at {path}: run node tools/forever_db.mjs', file=sys.stderr)
classes = {'warrior', 'paladin', 'hunter', 'rogue', 'priest', 'shaman', 'mage', 'warlock', 'druid',
           'human', 'dwarf', 'night_elf', 'gnome', 'skyborne', 'orc', 'undead', 'tauren', 'troll'}
cls = args[-1] if args[-1] in classes else None
terms = [a for a in args if a not in classes]
hits = []
for s in book.values():
    if cls and cls not in (s.get('class'), s.get('race')):
        continue
    for t in terms:
        if t.isdigit() and str(s['id']) == t:
            hits.append(s)
        elif not t.isdigit() and (t.lower() in s['name'].lower() if grep else s['name'].lower() == t.lower()):
            hits.append(s)
hits.sort(key=lambda s: (s['name'], s.get('level', 0), s['id']))
for s in hits:
    print(json.dumps(s, ensure_ascii=False))
    t = tips.get(str(s['id']))
    if t:
        for k in ('Description_lang', 'AuraDescription_lang'):
            if t.get(k):
                print(f'  {k.split("_")[0].lower()}: {t[k]}')
print(f'{len(hits)} spells', file=sys.stderr)
