#!/usr/bin/env python3
"""Regenerate the RESULTS.md table: every build in every headline test.

  python3 tools/results_table.py [--save results.json] [--compare old.json] [--at-build 1.60.1.70338]
  python3 tools/results_table.py --story ledger/

Runs `fsim run` (the release binary) for each column, prints the markdown
table, optionally saves the numbers, and with --compare lists every number
that moved by more than its share of noise. --story reads the saved
snapshots in a folder (one per change, named like 2-quick-fixes.json, in
order) and prints the "What moved and why" section: for each change, what it
was and the numbers it moved most.
"""
import json, os, subprocess, sys, tempfile, tomllib, glob

ROOT = os.environ.get('FSIM_ROOT') or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FSIM = os.path.join(ROOT, 'target/release/fsim')

# (column, scenario, level, metric, precision)
COLUMNS = [
    ('Boss DPS (60)', 'boss', None, 'dps', 0.01),
    ('Solo kills/h (20)', 'solo_1', 20, 'kills_per_hour', 0.015),
    ('(30)', 'solo_1', 30, 'kills_per_hour', 0.015),
    ('(60)', 'solo_1', None, 'kills_per_hour', 0.015),
    ('Hard doubles kills/h (60)', 'solo_2_hard', None, 'kills_per_hour', 0.02),
    ('Deaths/h', 'solo_2_hard', None, 'deaths_per_hour', 0.02),
    ('PvP breakeven (60)', 'pressure', None, 'breakeven', 0.03),
]
# The healing tests, for the healer builds: boss damage per second healed through.
HEAL_COLUMNS = [
    ('Dungeon (30)', 'heal_dungeon', 30, 'heal_breakeven', 0.015),
    ('Dungeon (60)', 'heal_dungeon', None, 'heal_breakeven', 0.015),
    ('Main tank (30)', 'heal_tank', 30, 'heal_breakeven', 0.015),
    ('Main tank (60)', 'heal_tank', None, 'heal_breakeven', 0.015),
    ('Raid (60)', 'heal_raid', None, 'heal_breakeven', 0.015),
]


def args():
    a = sys.argv[1:]
    get = lambda k: a[a.index(k) + 1] if k in a else None
    return get('--save'), get('--compare'), get('--at-build')


# What each saved snapshot changed (its file name without the number).
WHY = {
    'quick-fixes': 'Rage drains out of combat, Anger Management stops between pulls, gear below level 20 scales down, buffs before a fight cost time and mana (and come back after a death), Execute and Slam are used the way players use them',
    'play-like': 'The simulated player reacts like a typical player: about half a second to notice a proc, an interrupt chance or a new target, and the odd slip',
    'movement': 'Everyone moves: packs start unaware 30 to 35 yards away, casters get casts off before contact, melee walks in, Charge rushes, Frost mages kite, hunters back out of their dead zone, humanoids run at low health',
    'threat-tank-party': 'Threat decides who mobs attack, the boss test gets a real Protection warrior tank and the party\'s buffs, and the boss hits back',
    'enemy-mix': 'Leveling meets the open world\'s mix of creatures, some humanoids cast, and creature bonuses follow the target',
    'multi-target': 'DoTs spread to a second mob and Chain Lightning jumps',
    'crowd-control': 'Every class gets its crowd control, interrupts and escapes, and uses them on the add, on casters and when things go wrong; DoTs spread to a second mob and Chain Lightning jumps',
    'totems': 'Totems are units on the ground: a shaman puts them down where it stands, their buffs reach only the party nearby, and Searing Totem only hits a target within 20 yards; hunters freeze an add with a trap before the pull',
    'duels': 'Duels start fair: on the same tick, a few yards apart, whoever reacts first striking first',
    'talents': 'Talents that lengthen a spell\'s range, widen an area or make you faster now count, shamans Purge players\' buffs, and Improved Reincarnation brings you back stronger',
}

# Things a reader should know about a snapshot's numbers.
NOTE = {
    'quick-fixes': 'The Pact warlock and the two paladin hybrids fell this far because of a bug in this step (they recast their buffs before every pull, over and over), not because of the fixes; it was fixed with the crowd control step, where they come back.',
}


def story(folder):
    num = lambda f: int(f.split('-', 1)[0]) if f.split('-', 1)[0].isdigit() else -1
    files = sorted((f for f in os.listdir(folder) if f.endswith('.json')), key=lambda f: (num(f), f))
    snaps = [(f[:-5].split('-', 1)[1] if '-' in f else f[:-5], json.load(open(os.path.join(folder, f)))) for f in files]
    fmt = lambda col, v: f'{v:.2f}' if col == 'Deaths/h' else f'{v:.0f}'
    print('## What moved and why\n')
    for (_, old), (name, new) in zip(snaps, snaps[1:]):
        moves = []
        for key, row in new.items():
            for col, x in row.items():
                o = old.get(key, {}).get(col)
                if not o or not o['v'] or col == 'Deaths/h':
                    continue
                if x['v'] > o['hi'] or x['v'] < o['lo']:
                    moves.append((abs(x['v'] - o['v']) / abs(o['v']), key, col, o['v'], x['v']))
        moves.sort(reverse=True)
        print(f'**{WHY.get(name, name)}.**', f'{len(moves)} numbers moved beyond their ranges; the biggest:' if moves else 'Nothing moved beyond its range.')
        if name in NOTE:
            print(NOTE[name])
        print()
        for _, key, col, a, b in moves[:6]:
            cls, build = key.split('|', 1)
            who = build if f'({cls})' in build else f'{build} ({cls})'
            print(f'* {who}, {col}: {fmt(col, a)} to {fmt(col, b)} ({100 * (b - a) / abs(a):+.0f}%)')
        print()


KIND = {}  # result name -> (kind, role)


def builds():
    out = []
    for path in sorted(glob.glob(os.path.join(ROOT, 'builds/*/*.toml'))):
        d = tomllib.load(open(path, 'rb'))
        out.append((os.path.basename(os.path.dirname(path)), d['name'], os.path.relpath(path, ROOT), d.get('kind', 'experimental'), d.get('role', '')))
    # fsim adds the class to names that more than one build uses
    names = [n for _, n, _, _, _ in out]
    res = []
    for c, n, p, kind, role in out:
        name = f'{n} ({c})' if names.count(n) > 1 else n
        KIND[name] = (kind, role)
        res.append((c, name, p))
    return res


def run(scenario, level, precision, paths, at_build=None):
    with tempfile.NamedTemporaryFile(suffix='.json', delete=False) as t:
        out = t.name
    cmd = [FSIM, *(['--at-build', at_build] if at_build else []), 'run', scenario, *paths, '--precision', str(precision), '--json', out]
    # FSIM_OPTS="potions=1 x=2" runs every column with those assumptions
    for o in os.environ.get('FSIM_OPTS', '').split():
        cmd += ['--opt', o]
    if level:
        cmd += ['--level', str(level)]
    subprocess.run(cmd, cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    res = json.load(open(out))['results']
    os.unlink(out)
    return res


def main():
    if '--story' in sys.argv:
        story(sys.argv[sys.argv.index('--story') + 1])
        return
    save, compare, at_build = args()
    bs = builds()
    healers = [b for b in bs if KIND.get(b[1], ('', ''))[1] == 'healer']
    # FSIM_ONLY=healing runs just the healing table
    only = os.environ.get('FSIM_ONLY')
    cache = {}
    values = {}
    jobs = [(c, bs) for c in COLUMNS] if only != 'healing' else []
    jobs += [(c, healers) for c in HEAL_COLUMNS]
    for (col, scenario, level, metric, precision), who in jobs:
        key = (scenario, level, precision)
        if key not in cache:
            cache[key] = run(scenario, level, precision, [p for _, _, p in who], at_build)
        res = cache[key]
        for cls, name, _ in who:
            ms = {m['name']: m for m in res.get(name, [])}
            m = ms.get(metric)
            if m:
                v = m['median'] if m['kind'] == 'median' else m['mean']
                values.setdefault(f'{cls}|{name}', {})[col] = {'v': v, 'lo': m['lo'], 'hi': m['hi']}
    fmt = lambda col, v: f'{v:.2f}' if col == 'Deaths/h' else f'{v:.0f}'
    for kind, title in ([('standard', 'Standard specs'), ('experimental', 'Forever experiments')] if only != 'healing' else []):
        print(f'\n### {title}\n')
        print('| Class | Build | ' + ' | '.join(c for c, *_ in COLUMNS) + ' |')
        print('|---|---|' + '---:|' * len(COLUMNS))
        for cls, name, _ in bs:
            k, role = KIND.get(name, ('experimental', ''))
            if k != kind:
                continue
            row = values.get(f'{cls}|{name}', {})
            shown = name.replace(f' ({cls})', '') + (f' ({role})' if role else '')
            print(f'| {cls.capitalize()} | {shown} | ' + ' | '.join(fmt(c, row[c]['v']) if c in row else '–' for c, *_ in COLUMNS) + ' |')
    print('\n### Healers in the healing tests\n')
    print('| Class | Build | ' + ' | '.join(c for c, *_ in HEAL_COLUMNS) + ' |')
    print('|---|---|' + '---:|' * len(HEAL_COLUMNS))
    for cls, name, _ in healers:
        row = values.get(f'{cls}|{name}', {})
        print(f'| {cls.capitalize()} | {name.replace(f" ({cls})", "")} | ' + ' | '.join(fmt(c, row[c]['v']) if c in row else '–' for c, *_ in HEAL_COLUMNS) + ' |')
    if save:
        json.dump(values, open(save, 'w'), indent=1)
    if compare:
        old = json.load(open(compare))
        print('\nChanges beyond the confidence intervals:')
        for key, row in values.items():
            for col, x in row.items():
                o = old.get(key, {}).get(col)
                if not o:
                    continue
                # outside both intervals: a real change, not noise
                if x['v'] > o['hi'] or x['v'] < o['lo']:
                    pct = 100 * (x['v'] - o['v']) / abs(o['v']) if o['v'] else 0
                    print(f'  {key:42} {col:26} {fmt(col, o["v"]):>8} -> {fmt(col, x["v"]):>8} ({pct:+.0f}%)')


main()
