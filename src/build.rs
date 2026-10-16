//! Character builds: class, race, level, talents, gear kit and rotation,
//! loaded from a small TOML file and checked against the talent rules.

use crate::model::*;
use crate::vars::Stat;
use crate::world::parse_gear_stats;
use std::path::Path;
use toml::{Table, Value};

#[derive(Clone, Debug)]
pub struct Build {
    pub name: String,
    pub path: String,
    pub class: usize,
    pub race: usize,
    pub level: u32,
    pub talents: Vec<u8>,
    pub order: Vec<u16>,
    pub gear_id: String,
    pub gear: GearStats,
    pub rotation: String,
    pub opts: Vec<(u16, f64)>,
    pub warnings: Vec<String>,
}

impl Build {
    pub fn points_spent(&self) -> u32 {
        self.talents.iter().map(|&x| x as u32).sum()
    }
    pub fn tree_split(&self, w: &World) -> String {
        let c = &w.classes[self.class];
        let parts: Vec<String> = c
            .trees
            .iter()
            .map(|tree| {
                c.talents
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| &t.tree == tree)
                    .map(|(i, _)| self.talents[i] as u32)
                    .sum::<u32>()
                    .to_string()
            })
            .collect();
        parts.join("/")
    }
}

/// Interpolate a gear kit to a level.
pub fn gear_at(kit: &GearKit, level: u32) -> GearStats {
    let a = &kit.anchors;
    if a.len() == 1 || level <= a[0].0 {
        return a[0].1.clone();
    }
    for w in a.windows(2) {
        if level <= w[1].0 {
            let f = (level - w[0].0) as f64 / (w[1].0 - w[0].0) as f64;
            let (lo, hi) = (&w[0].1, &w[1].1);
            let mut out = GearStats::default();
            let mut keys: Vec<Stat> = lo.stats.iter().map(|x| x.0).collect();
            for (k, _) in &hi.stats {
                if !keys.contains(k) {
                    keys.push(*k);
                }
            }
            for k in keys {
                let l = lo.stats.iter().filter(|x| x.0 == k).map(|x| x.1).sum::<f64>();
                let h = hi.stats.iter().filter(|x| x.0 == k).map(|x| x.1).sum::<f64>();
                out.stats.push((k, l + f * (h - l)));
            }
            let lerp_w = |a: Option<Weapon>, b: Option<Weapon>| match (a, b) {
                (Some(x), Some(y)) => Some(Weapon {
                    min: x.min + f * (y.min - x.min),
                    max: x.max + f * (y.max - x.max),
                    speed: if f < 0.5 { x.speed } else { y.speed },
                    wtype: if f < 0.5 { x.wtype } else { y.wtype },
                    school: if f < 0.5 { x.school } else { y.school },
                }),
                (x, y) => if f < 0.5 { x } else { y },
            };
            out.mh = lerp_w(lo.mh, hi.mh);
            out.oh = lerp_w(lo.oh, hi.oh);
            out.ranged = lerp_w(lo.ranged, hi.ranged);
            out.shield = match (lo.shield, hi.shield) {
                (Some(x), Some(y)) => Some((x.0 + f * (y.0 - x.0), x.1 + f * (y.1 - x.1))),
                (x, y) => if f < 0.5 { x } else { y },
            };
            return out;
        }
    }
    a[a.len() - 1].1.clone()
}

/// Check talent legality: point budget, ranks, tree depth and prerequisites.
pub fn validate_talents(w: &World, class: usize, talents: &[u8], level: u32) -> Vec<String> {
    let c = &w.classes[class];
    let mut errs = Vec::new();
    let total: u32 = talents.iter().map(|&x| x as u32).sum();
    let budget = level.saturating_sub(9);
    if total > budget {
        errs.push(format!("{total} talent points spent but a level {level} character has {budget}"));
    }
    for (i, t) in c.talents.iter().enumerate() {
        let r = talents[i] as u32;
        if r == 0 {
            continue;
        }
        if r > t.ranks {
            errs.push(format!("{}: {r} points but it has {} ranks", t.name, t.ranks));
        }
        let below: u32 = c
            .talents
            .iter()
            .enumerate()
            .filter(|(_, o)| o.tree == t.tree && o.points < t.points)
            .map(|(j, _)| talents[j] as u32)
            .sum();
        if below < t.points {
            errs.push(format!(
                "{}: needs {} points in {} above its row, has {below}",
                t.name, t.points, t.tree
            ));
        }
        if let Some(req) = t.requires {
            let rq = &c.talents[req as usize];
            if (talents[req as usize] as u32) < rq.ranks {
                errs.push(format!("{}: requires {} at {}/{} first", t.name, rq.name, rq.ranks, rq.ranks));
            }
        }
    }
    errs
}

/// A legal levelling order for a talent build when the file gives none:
/// the tree with the most points first, shallow rows before deep ones,
/// file order within a row.
pub fn auto_order(w: &World, class: usize, target: &[u8]) -> Vec<u16> {
    let c = &w.classes[class];
    let tree_pts = |tree: &str| -> u32 {
        c.talents.iter().enumerate().filter(|(_, t)| t.tree == tree).map(|(i, _)| target[i] as u32).sum()
    };
    let mut trees: Vec<(u32, usize)> = c.trees.iter().enumerate().map(|(k, t)| (tree_pts(t), k)).collect();
    trees.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let prio = |tree: &str| trees.iter().position(|&(_, k)| c.trees[k] == tree).unwrap_or(99);
    let mut cur = vec![0u8; c.talents.len()];
    let mut order = Vec::new();
    loop {
        let mut best: Option<(usize, u32, usize)> = None;
        for (i, t) in c.talents.iter().enumerate() {
            if cur[i] >= target[i] {
                continue;
            }
            let below: u32 = c
                .talents
                .iter()
                .enumerate()
                .filter(|(_, o)| o.tree == t.tree && o.points < t.points)
                .map(|(j, _)| cur[j] as u32)
                .sum();
            if below < t.points {
                continue;
            }
            if let Some(req) = t.requires {
                if (cur[req as usize] as u32) < c.talents[req as usize].ranks {
                    continue;
                }
            }
            let key = (prio(&t.tree), t.points, i);
            if best.map_or(true, |b| key < b) {
                best = Some(key);
            }
        }
        match best {
            Some((_, _, i)) => {
                cur[i] += 1;
                order.push(i as u16);
            }
            None => break,
        }
    }
    order
}

/// Replay a level-by-level order and report the first illegal step.
pub fn validate_order(w: &World, class: usize, order: &[u16]) -> Vec<String> {
    let c = &w.classes[class];
    let mut cur = vec![0u8; c.talents.len()];
    let mut errs = Vec::new();
    for (step, &ti) in order.iter().enumerate() {
        cur[ti as usize] += 1;
        let level = 10 + step as u32;
        let e = validate_talents(w, class, &cur, level);
        if !e.is_empty() {
            errs.push(format!("level {level} ({}): {}", c.talents[ti as usize].name, e.join("; ")));
            break;
        }
    }
    errs
}

pub fn load_build(w: &World, path: &Path, level_override: Option<u32>) -> Result<Build, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let t: Table = toml::from_str(&src).map_err(|e| format!("{}: {e}", path.display()))?;
    build_from_table(w, &t, &path.display().to_string(), level_override)
}

pub fn build_from_table(w: &World, t: &Table, path: &str, level_override: Option<u32>) -> Result<Build, String> {
    let get = |k: &str| t.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
    let class_id = get("class").ok_or_else(|| format!("{path}: needs `class`"))?;
    let class = w.class_index(&class_id).ok_or_else(|| format!("{path}: unknown class '{class_id}'"))?;
    let race_id = get("race").ok_or_else(|| format!("{path}: needs `race`"))?;
    let race = w.race_index(&race_id).ok_or_else(|| format!("{path}: unknown race '{race_id}'"))?;
    let mut warnings = Vec::new();
    if !w.races[race].classes.is_empty() && !w.races[race].classes.contains(&class_id) {
        return Err(format!("{path}: {} cannot be a {}", w.races[race].name, w.classes[class].name));
    }
    let file_level = t.get("level").and_then(|v| v.as_integer()).unwrap_or(60) as u32;
    let level = level_override.unwrap_or(file_level);
    let c = &w.classes[class];
    let mut talents = vec![0u8; c.talents.len()];
    if let Some(tt) = t.get("talents").and_then(|v| v.as_table()) {
        for (k, v) in tt {
            let i = *c.talent_index.get(k).ok_or_else(|| format!("{path}: unknown {} talent '{k}'", c.id))?;
            talents[i as usize] = v.as_integer().ok_or_else(|| format!("{path}: talents.{k} must be a number"))? as u8;
        }
    }
    let mut order = Vec::new();
    if let Some(Value::Array(a)) = t.get("order") {
        for v in a {
            let k = v.as_str().ok_or_else(|| format!("{path}: order entries are talent ids"))?;
            order.push(*c.talent_index.get(k).ok_or_else(|| format!("{path}: order: unknown talent '{k}'"))?);
        }
        let e = validate_order(w, class, &order);
        if !e.is_empty() {
            return Err(format!("{path}: illegal talent order: {}", e.join("; ")));
        }
        let mut from_order = vec![0u8; c.talents.len()];
        for &i in &order {
            from_order[i as usize] += 1;
        }
        if talents.iter().all(|&x| x == 0) {
            talents = from_order.clone();
        } else if talents != from_order {
            warnings.push("`order` and `talents` disagree; using `talents` (order is used when scanning levels)".into());
        }
    }
    // Trim to the level's budget using the order if the level is lower than the build.
    let budget = level.saturating_sub(9) as usize;
    if talents.iter().map(|&x| x as usize).sum::<usize>() > budget {
        if order.is_empty() {
            let errs = validate_talents(w, class, &talents, 60);
            if !errs.is_empty() {
                return Err(format!("{path}: illegal talents: {}", errs.join("; ")));
            }
            order = auto_order(w, class, &talents);
            warnings.push(format!(
                "level {level}: no `order` in the file, so points were taken main tree first, top rows first ({} of {})",
                budget,
                talents.iter().map(|&x| x as usize).sum::<usize>()
            ));
        }
        talents = vec![0u8; c.talents.len()];
        for &i in order.iter().take(budget) {
            talents[i as usize] += 1;
        }
    }
    let errs = validate_talents(w, class, &talents, level);
    if !errs.is_empty() {
        return Err(format!("{path}: illegal talents: {}", errs.join("; ")));
    }
    let gear_id = get("gear").ok_or_else(|| format!("{path}: needs `gear` (a kit from data/base/gear.toml)"))?;
    let kit = w.gear.get(&gear_id).ok_or_else(|| format!("{path}: unknown gear kit '{gear_id}'"))?;
    let mut gear = gear_at(kit, level);
    if let Some(ov) = t.get("gear_override").and_then(|v| v.as_table()) {
        let extra = parse_gear_stats(ov, &format!("{path}.gear_override"))?;
        let replace = ov.get("replace").and_then(|v| v.as_bool()).unwrap_or(false);
        for (st, x) in extra.stats {
            if replace {
                gear.stats.retain(|(s, _)| *s != st);
            }
            gear.stats.push((st, x));
        }
        if extra.mh.is_some() {
            gear.mh = extra.mh;
        }
        if extra.oh.is_some() {
            gear.oh = extra.oh;
        }
        if extra.ranged.is_some() {
            gear.ranged = extra.ranged;
        }
        if extra.shield.is_some() {
            gear.shield = extra.shield;
            gear.oh = None;
        }
    }
    if gear.oh.is_some() && !c.can_dual_wield {
        warnings.push(format!("{} cannot dual wield; off-hand ignored", c.name));
        gear.oh = None;
    }
    let rotation = get("rotation").ok_or_else(|| format!("{path}: needs `rotation`"))?;
    if !c.rotations.contains_key(&rotation) {
        return Err(format!(
            "{path}: unknown rotation '{rotation}' for {} (have: {})",
            c.id,
            c.rotations.keys().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    let mut opts = Vec::new();
    if let Some(ot) = t.get("opts").and_then(|v| v.as_table()) {
        for (k, v) in ot {
            let i = w.opt_index(k).ok_or_else(|| format!("{path}: opts: unknown option '{k}'"))?;
            let x = v.as_float().or_else(|| v.as_integer().map(|i| i as f64)).or_else(|| v.as_bool().map(|b| b as u8 as f64));
            opts.push((i, x.ok_or_else(|| format!("{path}: opts.{k} must be a number"))?));
        }
    }
    Ok(Build {
        name: get("name").unwrap_or_else(|| path.to_string()),
        path: path.to_string(),
        class,
        race,
        level,
        talents,
        order,
        gear_id,
        gear,
        rotation,
        opts,
        warnings,
    })
}
