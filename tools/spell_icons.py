#!/usr/bin/env python3
"""Give every spell in the data its in-game icon.

  python3 tools/spell_icons.py [--check]

Covers the class files, racial and common spells, and spells a patch adds.
For each spell table the icon comes from the spellbook
(data/reference/spellbook.json, built by tools/forever_db.mjs from the game's
SpellMisc and ManifestInterfaceData tables) through the spell's game ids; a
talent spell without ids takes its talent's icon. The script writes
`icon = "..."` under the spell's name line, or updates it. Spells with no
game id (item-based poisons, summons, potions) use the hand-set icons below.
A spell a patch only changes keeps the icon from base. --check only reports.
"""
import glob, json, os, re, sys, tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
book = json.load(open(os.path.join(ROOT, 'data/reference/spellbook.json')))['spells']
check = '--check' in sys.argv

# Spells the data has no game id for: their Classic icons, set by hand.
MANUAL = {
    'druid.caster_form': 'classicon_druid',
    'druid.swiftmend_heal': 'inv_relics_idolofrejuvenation',
    'mage.evocation': 'spell_nature_purge',
    'mage.counterspell': 'spell_frost_iceshock',
    'rogue.instant_poison_mh': 'ability_poisons',
    'rogue.instant_poison_oh': 'ability_poisons',
    'rogue.deadly_poison_mh': 'ability_rogue_dualweild',
    'rogue.deadly_poison_oh': 'ability_rogue_dualweild',
    'warlock.summon_imp': 'spell_shadow_summonimp',
    'warlock.summon_voidwalker': 'spell_shadow_summonvoidwalker',
    'warlock.summon_succubus': 'spell_shadow_summonsuccubus',
    'warlock.summon_felhunter': 'spell_shadow_summonfelhunter',
    'common.shoot': 'ability_shootwand',
    'common.healing_potion': 'inv_potion_54',
    'common.mana_potion': 'inv_potion_76',
}


def ids_of(sp):
    out = []
    if isinstance(sp.get('id'), int):
        out.append(sp['id'])
    for r in sp.get('ranks', []) or []:
        if isinstance(r, dict) and isinstance(r.get('id'), int):
            out.append(r['id'])
    return out


def dig(d, path):
    for k in path:
        d = d.get(k, {}) if isinstance(d, dict) else {}
    return d


def base_spell(owner):
    """The spell as base data defines it, for spells a patch changes."""
    kind, who, key = owner
    if kind == 'classes':
        f = os.path.join(ROOT, 'data/base/classes', f'{who}.toml')
        return dig(tomllib.load(open(f, 'rb')), ['spells', key]) if os.path.exists(f) else {}
    if kind == 'races':
        return dig(tomllib.load(open(os.path.join(ROOT, 'data/base/races.toml'), 'rb')), ['races', who, 'spells', key])
    return dig(tomllib.load(open(os.path.join(ROOT, 'data/base/common.toml'), 'rb')), ['common', 'spells', key])


def spell_tables(path, data):
    """(header path, owner, spell, talents) for each spell table in a file."""
    rel = os.path.relpath(path, ROOT)
    if rel.startswith('data/base/classes/'):
        cls = os.path.basename(path)[:-5]
        for k, sp in data.get('spells', {}).items():
            yield f'spells.{k}', ('classes', cls, k), sp, data.get('talents', {})
        return
    for cls, c in data.get('classes', {}).items():
        for k, sp in c.get('spells', {}).items():
            yield f'classes.{cls}.spells.{k}', ('classes', cls, k), sp, c.get('talents', {})
    for race, r in data.get('races', {}).items():
        for k, sp in r.get('spells', {}).items():
            yield f'races.{race}.spells.{k}', ('races', race, k), sp, {}
    for k, sp in data.get('common', {}).get('spells', {}).items():
        yield f'common.spells.{k}', ('common', 'common', k), sp, {}


files = sorted(glob.glob(os.path.join(ROOT, 'data/base/classes/*.toml'))) + [
    os.path.join(ROOT, 'data/base/races.toml'), os.path.join(ROOT, 'data/base/common.toml')] + sorted(glob.glob(os.path.join(ROOT, 'data/patches/*.toml')))
changed = 0
missing = []
for path in files:
    text = open(path).read()
    data = tomllib.loads(text)
    patch = '/patches/' in path
    lines = text.split('\n')
    for head, owner, sp, talents in spell_tables(path, data):
        label = f'{owner[1]}.{owner[2]}'
        base = base_spell(owner) if patch else {}
        if patch and base.get('icon'):
            continue  # a change to a spell base already gives an icon
        icon = None
        for i in ids_of(sp) + ids_of(base):
            icon = (book.get(str(i)) or {}).get('icon')
            if icon:
                break
        if not icon and owner[0] == 'races':
            # racials have no ids in the data: match the game's racial by name
            race = 'skyborne' if owner[1] in ('high_order', 'windshaper') else owner[1]
            name = sp.get('name') or base.get('name')
            icon = next((v.get('icon') for v in book.values() if v.get('race') == race and v.get('name') == name and v.get('icon')), None)
        talent = sp.get('talent') or base.get('talent')
        if not icon and talent:
            icon = talents.get(talent, {}).get('icon')
            if not icon and owner[0] == 'classes':
                f = os.path.join(ROOT, 'data/base/classes', f'{owner[1]}.toml')
                icon = dig(tomllib.load(open(f, 'rb')), ['talents', talent]).get('icon')
        icon = icon or MANUAL.get(label)
        if not icon:
            if not sp.get('icon'):
                missing.append(label)
            continue
        if sp.get('icon') == icon:
            continue
        at = next((n for n, l in enumerate(lines) if l.strip() == f'[{head}]'), None)
        if at is None:
            missing.append(f'{label} (header not found)')
            continue
        end = next((n for n in range(at + 1, len(lines)) if lines[n].startswith('[')), len(lines))
        existing = next((n for n in range(at + 1, end) if re.match(r'icon\s*=', lines[n])), None)
        line = f'icon = "{icon}"'
        if existing is not None:
            lines[existing] = line
        else:
            nm = next((n for n in range(at + 1, end) if re.match(r'name\s*=', lines[n])), at)
            lines.insert(nm + 1, line)
        changed += 1
    if not check:
        new = '\n'.join(lines)
        if new != text:
            open(path, 'w').write(new)

print(f'{"would set" if check else "set"} {changed} icons')
if missing:
    print('no icon found for:', ', '.join(missing))
