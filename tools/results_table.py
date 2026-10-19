#!/usr/bin/env python3
"""Regenerate the RESULTS.md table: every build in every headline test.

  python3 tools/results_table.py [--save results.json] [--compare old.json] [--at-build 1.60.1.70338]
  python3 tools/results_table.py --story ledger/
  FSIM_CLASS=mage python3 tools/results_table.py --compare old.json  (one class's builds)

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
    ('You strike first (60)', 'strike_first', None, 'score', 0.03),
    ('They strike first (60)', 'struck_first', None, 'score', 0.03),
]
# Columns shown as a share (open-world PvP scores: a win 1, a draw half).
SHARE = {'You strike first (60)', 'They strike first (60)'}
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
    'kiting-and-skirmish': 'Casters and hybrids step out of a rooted or frozen enemy\'s reach before casting, roots break on damage, and two open-world PvP tests arrive: you strike first, and they strike first',
    'hybrid-gear': 'Four hybrid gear kits from Forever\'s own items (melee gear with spell damage and healing), and the Enhancement shaman, Retribution paladin and Shockadin wear theirs',
    'openers-and-boss': 'Opening moves wait out a reaction, a stun or a stance swap instead of being skipped (Fury Charges again, hunters Mark the boss, the tank charges in and goes back to Defensive Stance), damage dealers hold their white swings while near the tank\'s threat, boss damage per second counts the whole fight, and nobody plays an opener when ambushed',
    'ambush': 'Open-world fights become real ambushes: whoever is jumped keeps only long buffs (no fresh shields, charge shields or seals) with its cooldowns ready, rogues and druids striking first walk in stealthed and open from Stealth, a waiting player notices an enemy walking up at 20 yards, and totems go down with the fight',
    'rest-and-party': 'The boss test\'s party becomes a typical group by default (the tank\'s Battle Shout, a priest\'s Fortitude, a mage\'s Arcane Intellect, and one paladin blessing for the Alliance or a shaman\'s air and earth totems for the Horde), with no buffs or every buff a click away in Settings, and pets get the party\'s buffs; potions are drunk in group fights by default; eating and drinking count regeneration, mages drink conjured water, and pets keep their wounds between pulls (hunters Mend Pet, warlocks Health Funnel)',
    'casting': 'Casting follows the original game: a hit pushes a cast back 1, 0.8, 0.6, 0.4 and then 0.2 seconds, never past its full length, and shortens a channel by the same (the later, gentler rules are a setting); Improved Channeling resists it for Arcane Missiles; a cast that lands with its target out of range or in a bow\'s dead zone fails, a channel breaks once its target is out of range, finishing a cast restarts the melee swing, and Frost Nova\'s freeze no longer stops a cast',
    'attack-tables': 'Attack tables follow the original game: debuffs, crowd control, interrupts and taunts can miss, be dodged or parried, or be resisted (once per cast and target; a Charge\'s stun always lands), spell crit isn\'t lowered by the target\'s level, higher-level targets partly resist direct spell damage, glancing blows follow weapon skill, a parry speeds up the next swing, stunned or feared targets can\'t dodge, Overpower can\'t be dodged, and a queued Heroic Strike drops the off-hand\'s dual-wield miss',
    'schools-and-cc': 'Spells carry their real schools, so an interrupt locks out only the school it caught and Silence stops buffs, totems and blessings too; Counterspell, a pet\'s Growl and the Voidwalker\'s Torment need range (pets must reach an add before taunting it); Improved Counterspell silences; feared units wander and are held by roots and slows; Death Coil horrifies instead of fearing; Intimidating Shout\'s target cowers while others nearby run; and the ambusher\'s Kidney Shot keeps its own diminishing returns, so it lasts its full 6 seconds',
    'tools': 'New tools: bandages between pulls when you\'d eat anyway (Linen at level 1 up to Heavy Runecloth at 54), which speeds up leveling most for rogues and warriors; food and drink go in together once you sit down; cures for every class that has them (Cleanse, Dispel Magic, Remove Curse and the rest), with debuffs carrying their dispel types; the healing tests\' bosses gain a Mortal Strike that halves healing for 5 seconds and, in the dungeon, a Shadow Word: Pain that priests and paladins take off while their mana allows; and Preparation, Entrapment, Arcane Shielding, Guardian\'s Favor, Litany of Light, Purifying Power\'s cheaper cures and Vile Poisons\' sturdier poison now work',
    'rotation-support': 'Rotation support: a line that keeps a debuff or buff up counts as upkeep (which a typical player reapplies a little late) only when it watches the aura being there, not its stacks, so building Arcane Blast stacks and Sunder Armor no longer wait; spells on a shared cooldown aren\'t upkeep either; and your side\'s crowd control holds: while another enemy fights, you, your pet, your swings and your single-target spells leave a Polymorphed, Sapped, trapped or seduced enemy alone (hunter pets no longer break their own Freezing Trap). Emergency lines, which stop a cast to save you, are ready for the rotation pass',
    'rest-and-emergencies': 'Resting reads the next pull: every build eats below 85% and drinks below 70% before a double or a pull of tougher mobs, and keeps its old thresholds for single mobs, so deaths in hard doubles fall by half or more; and life-saving buttons (potions, Healthstone, Ice Block, Divine Shield, Evasion, Barkskin, Lay on Hands and the like) are emergencies: taken after a reaction, never late, even mid-cast. Paladins bubble at 30% against players',
    'class-rotations': 'Every class\'s rotations, reworked from the review\'s logs: rogues open with Sap and Cheap Shot and use Vanish, Sprint and Blind; mages stop walking in for Fire Blast, step back after Frost Nova and switch schools when interrupted; warlocks Life Tap deeper on bosses, fear on contact and stop eating their own Immolate; warriors stance-dance for Intercept, Pummel and Disarm; druids gain Innervate and stop flipping forms; paladins keep Consecration for packs and heal behind their bubble; priests stop re-shielding and pace their healing; shamans open from range and keep buff totems for bosses and packs; hunters Scatter only as a peel and stop breaking their own traps. Light\'s Vigil, Vampiric Embrace and Divine Aegis now reach the party, and Inner Focus covers Prayer of Healing',
    'rotation-engine': 'What the rotation pass asked of the engine: a rotation\'s range can be a formula (melee Survival keeps a hunter\'s range against players, more than doubling its open-world scores), the automatic range counts damage lines only (Destruction and Shadow stand at their talented reach again), a line can cap its rank (Elemental casts Lightning Bolt rank 8 on monsters, +7% on the boss), you and your pet stop auto-attacking a target your own crowd control holds, emergencies start their reaction during the global cooldown, openers skip a line whose condition fails instead of waiting for it, totems can\'t be feared, regeneration while casting is capped at all of it, and a jumped priest no longer keeps Weakened Soul without the shield',
    'statistics': 'Statistics and speed: scores and win rates get ranges that stay inside 0 to 100%, breakeven comparisons take the difference of the medians with a paired bootstrap, the healing tests search on a finer grid (12 steps), and the engine runs 11 to 14% faster. The numbers themselves don\'t move',
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
    fmt = lambda col, v: f'{v:.2f}' if col == 'Deaths/h' else f'{100 * v:.0f}%' if col in SHARE else f'{v:.0f}'
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
RUN_NAME = {}  # result name -> its name in this run


def builds():
    out = []
    for path in sorted(glob.glob(os.path.join(ROOT, 'builds/*/*.toml'))):
        d = tomllib.load(open(path, 'rb'))
        out.append((os.path.basename(os.path.dirname(path)), d['name'], os.path.relpath(path, ROOT), d.get('kind', 'experimental'), d.get('role', '')))
    # fsim adds the class to names that more than one build uses; the rows
    # keep the names of a run over every build, even when FSIM_CLASS=<class>
    # runs just one class's builds
    names = [n for _, n, _, _, _ in out]
    only = os.environ.get('FSIM_CLASS')
    ran = [n for c, n, _, _, _ in out if not only or c == only]
    res = []
    for c, n, p, kind, role in out:
        name = f'{n} ({c})' if names.count(n) > 1 else n
        KIND[name] = (kind, role)
        if only and c != only:
            continue
        RUN_NAME[name] = f'{n} ({c})' if ran.count(n) > 1 else n
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
    jobs += [(c, healers) for c in HEAL_COLUMNS] if healers else []
    for (col, scenario, level, metric, precision), who in jobs:
        key = (scenario, level, precision)
        if key not in cache:
            cache[key] = run(scenario, level, precision, [p for _, _, p in who], at_build)
        res = cache[key]
        for cls, name, _ in who:
            ms = {m['name']: m for m in res.get(RUN_NAME.get(name, name), [])}
            m = ms.get(metric)
            if m:
                v = m['median'] if m['kind'] == 'median' else m['mean']
                values.setdefault(f'{cls}|{name}', {})[col] = {'v': v, 'lo': m['lo'], 'hi': m['hi']}
    fmt = lambda col, v: f'{v:.2f}' if col == 'Deaths/h' else f'{100 * v:.0f}%' if col in SHARE else f'{v:.0f}'
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
