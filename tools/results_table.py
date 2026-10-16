#!/usr/bin/env python3
"""Regenerate the RESULTS.md table: every build in every headline test.

  python3 tools/results_table.py [--save results.json] [--compare old.json] [--at-build 1.60.1.70338]

Runs `fsim run` (the release binary) for each column, prints the markdown
table, optionally saves the numbers, and with --compare lists every number
that moved by more than its share of noise.
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


def args():
    a = sys.argv[1:]
    get = lambda k: a[a.index(k) + 1] if k in a else None
    return get('--save'), get('--compare'), get('--at-build')


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
    if level:
        cmd += ['--level', str(level)]
    subprocess.run(cmd, cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    res = json.load(open(out))['results']
    os.unlink(out)
    return res


def main():
    save, compare, at_build = args()
    bs = builds()
    paths = [p for _, _, p in bs]
    cache = {}
    values = {}
    for col, scenario, level, metric, precision in COLUMNS:
        key = (scenario, level, precision)
        if key not in cache:
            cache[key] = run(scenario, level, precision, paths, at_build)
        res = cache[key]
        for cls, name, _ in bs:
            ms = {m['name']: m for m in res.get(name, [])}
            m = ms.get(metric)
            if m:
                v = m['median'] if m['kind'] == 'median' else m['mean']
                values.setdefault(f'{cls}|{name}', {})[col] = {'v': v, 'lo': m['lo'], 'hi': m['hi']}
    fmt = lambda col, v: f'{v:.2f}' if col == 'Deaths/h' else f'{v:.0f}'
    for kind, title in [('standard', 'Standard specs'), ('experimental', 'Forever experiments')]:
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
