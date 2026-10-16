//! JSON interface used by the web front end (through WebAssembly or the
//! local `fsim serve` server). One entry point: `handle(request) -> response`,
//! both JSON strings. Data files and example builds are embedded at build
//! time, so this needs no file system.
//!
//! Requests (`op`):
//! * `catalog` - everything the UI needs to draw pickers and talent trees.
//! * `sheet` {build} - validate a build and return its character sheet.
//! * `start` {scenario, builds, opponent?, level?, opts?, precision?, max_reps?, seed?}
//!   -> {id, max_reps}. Starts a Monte Carlo session; nothing runs yet.
//! * `step` {id, reps} - run up to `reps` more replications per build and
//!   report progress; call until `finished`.
//! * `run` {id, from, count} - run replications `from..from+count` and return
//!   their raw values without keeping them. The page sends these to several
//!   Web Workers at once, each holding the same session.
//! * `add` {id, chunk} - hand a `run` response to the session that keeps the
//!   results; it reports progress like `step`.
//! Convergence is checked at fixed replication counts (`min_reps`, then
//! every 16), so a session stops at the same point however the work was
//! split between workers, and gives the same result.
//! * `result` {id, pairs?, columns?} - summaries, intervals and paired
//!   comparisons: build `b` against build `a` for each `[a, b]` in `pairs`
//!   (indices into `builds`), every ordered pair for `pairs: "all"`, or every
//!   build against the first when absent. `columns: [metric]` adds each
//!   build's per-replication values of those metrics.
//! * `pair` {metric, a: [values], b: [values]} - the same-seed difference
//!   b - a over the replications both have. Replication r of a build always
//!   gives the same values (same build, settings and seed), so columns kept
//!   from an earlier run pair exactly with a new run's.
//! * `drop` {id} - forget a session.
//! Every request may carry `patches: [{name, text}]` (TOML overlays) to test
//! data changes; the world is rebuilt when they change.

use crate::build::{build_from_table, Build};
use crate::engine::Sim;
use crate::mc::{self, McCfg, McResult, Variant};
use crate::model::*;
use crate::scenario::{run_rep, run_rep_traced, RepInput, Values};
use crate::validate::modelled_talents;
use crate::vars::Stat;
use crate::world::{load_world_mem, MemData};
use serde_json::{json, Map, Value as J};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}

/// Replications for every variant: [variant][replication].
type Chunk = Vec<Vec<Values>>;

struct Session {
    world: Arc<World>,
    scenario: String,
    variants: Vec<Variant>,
    /// Replications 0..n, in order.
    values: Chunk,
    /// Chunks that arrived ahead of `values`, keyed by first replication.
    pending: BTreeMap<usize, Chunk>,
    /// Replication count at the last convergence check, and its precision.
    checked: usize,
    precision: Vec<f64>,
    cfg: McCfg,
    converged: bool,
    finished: bool,
}

struct State {
    world_key: String,
    world: Option<Arc<World>>,
    sessions: HashMap<u64, Session>,
    next_id: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn err(msg: impl std::fmt::Display) -> String {
    json!({ "error": msg.to_string() }).to_string()
}

/// Handle one request. Never panics on bad input; errors come back as {"error": ...}.
pub fn handle(req: &str) -> String {
    let v: J = match serde_json::from_str(req) {
        Ok(v) => v,
        Err(e) => return err(format!("bad request: {e}")),
    };
    let mut guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
    let st = guard.get_or_insert_with(|| State { world_key: String::new(), world: None, sessions: HashMap::new(), next_id: 1 });
    let world = match world_for(st, &v) {
        Ok(w) => w,
        Err(e) => return err(e),
    };
    let op = v.get("op").and_then(|x| x.as_str()).unwrap_or("");
    let out = match op {
        "catalog" => Ok(catalog(&world)),
        "sheet" => sheet(&world, &v),
        "start" => start(st, world, &v),
        "step" => step(st, &v),
        "run" => run_chunk(st, &v),
        "add" => add(st, &v),
        "result" => result(st, &v),
        "pair" => pair(&v),
        "trace" => trace(&world, &v),
        "drop" => {
            if let Some(id) = v.get("id").and_then(|x| x.as_u64()) {
                st.sessions.remove(&id);
            }
            Ok(json!({ "ok": true }))
        }
        _ => Err(format!("unknown op '{op}'")),
    };
    match out {
        Ok(j) => j.to_string(),
        Err(e) => err(e),
    }
}

fn world_for(st: &mut State, v: &J) -> Result<Arc<World>, String> {
    let patches: Vec<(String, String)> = v
        .get("patches")
        .and_then(|p| p.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| Some((p.get("name")?.as_str()?.to_string(), p.get("text")?.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let key = patches.iter().map(|(n, t)| format!("{n}\u{1}{t}")).collect::<Vec<_>>().join("\u{2}");
    if let Some(w) = &st.world {
        if st.world_key == key {
            return Ok(w.clone());
        }
    }
    let files: Vec<(String, String)> = embedded::DATA_FILES.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect();
    let w = load_world_mem(&MemData { files: &files, at_build: None, extra: &patches, skip_patches: false })?;
    let w = Arc::new(w);
    st.world = Some(w.clone());
    st.world_key = key;
    Ok(w)
}

// ---------------------------------------------------------------- catalog

fn catalog(w: &World) -> J {
    let classes: Vec<J> = w
        .classes
        .iter()
        .enumerate()
        .map(|(ci, c)| {
            let modelled = modelled_talents(w, ci);
            let talents: Vec<J> = c
                .talents
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    json!({
                        "id": t.id, "name": t.name, "tree": t.tree, "points": t.points, "ranks": t.ranks,
                        "requires": t.requires.map(|r| c.talents[r as usize].id.clone()),
                        "text": t.text, "modelled": modelled[i], "icon": t.icon,
                    })
                })
                .collect();
            let ci = w.class_index(&c.id);
            // each line with its condition in plain English, for "how this build played"
            let lines = |list: &[Action]| -> Vec<J> {
                list.iter().map(|a| {
                    let sp = &w.spells[a.spell as usize];
                    json!({
                        "spell": sp.id, "name": sp.name,
                        "when": a.cond.as_ref().and_then(|e| crate::explain::condition(w, ci, e)),
                        // what the spell itself needs (Victory Rush after a kill)
                        "needs": sp.requires.as_ref().and_then(|e| crate::explain::condition(w, ci, e)),
                    })
                }).collect()
            };
            let rotations: Vec<J> = c.rotations.values().map(|r| json!({
                "id": r.id, "desc": r.desc, "lines": lines(&r.actions), "rest": lines(&r.rest),
                "precombat": r.precombat.iter().map(|a| w.spells[a.spell as usize].name.clone()).collect::<Vec<_>>(),
                "opener": r.opener.iter().map(|a| w.spells[a.spell as usize].name.clone()).collect::<Vec<_>>(),
            })).collect();
            let races: Vec<&str> = w.races.iter().filter(|r| r.classes.is_empty() || r.classes.contains(&c.id)).map(|r| r.id.as_str()).collect();
            json!({
                "id": c.id, "name": c.name, "icon": c.icon, "trees": c.trees, "talents": talents, "rotations": rotations, "races": races,
                "resources": c.resources.iter().map(|r| r.name()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let races: Vec<J> = w
        .races
        .iter()
        .map(|r| json!({ "id": r.id, "name": r.name, "icon": r.icon, "faction": r.faction, "classes": r.classes, "passives": r.passives.iter().map(|p| p.name.clone()).collect::<Vec<_>>() }))
        .collect();
    let gear: Vec<J> = w.gear.values().map(|g| json!({ "id": g.id, "desc": g.desc, "levels": g.anchors.iter().map(|a| a.0).collect::<Vec<_>>() })).collect();
    let scenarios: Vec<J> = w
        .scenarios
        .values()
        .map(|s| {
            let kind = match s.kind {
                ScenarioKind::Leveling { .. } => "leveling",
                ScenarioKind::Boss { .. } => "boss",
                ScenarioKind::Duel { .. } => "duel",
                ScenarioKind::Pressure { .. } => "pressure",
            };
            json!({ "id": s.id, "desc": s.desc, "kind": kind, "primary": s.primary })
        })
        .collect();
    let options: Vec<J> = w
        .opts
        .iter()
        .map(|o| {
            json!({
                "name": o.name, "default": o.default, "desc": o.desc, "label": o.label, "help": o.help, "kind": o.kind,
                "unit": o.unit, "source": o.source, "cite": o.cite, "applies": o.applies,
            })
        })
        .collect();
    let presets: Vec<J> = embedded::BUILD_FILES
        .iter()
        .filter_map(|(path, text)| {
            let t: toml::Table = toml::from_str(text).ok()?;
            let mut b = serde_json::to_value(&t).ok()?;
            let note: Vec<&str> = text.lines().take_while(|l| l.starts_with('#')).map(|l| l.trim_start_matches('#').trim()).collect();
            b["file"] = J::from(*path);
            b["note"] = J::from(note.join(" "));
            // the build's points at its own level, also when the file gives
            // them only as a leveling order
            if let Ok(built) = build_from_table(w, &t, path, None) {
                let c = &w.classes[built.class];
                let talents: Map<String, J> = c.talents.iter().enumerate().filter(|(i, _)| built.talents[*i] > 0).map(|(i, x)| (x.id.clone(), J::from(built.talents[i]))).collect();
                b["talents"] = J::Object(talents);
            }
            Some(b)
        })
        .collect();
    json!({
        "data_build": w.meta.data_build,
        "patches": w.meta.patches,
        "classes": classes, "races": races, "gear": gear, "scenarios": scenarios, "options": options, "presets": presets,
    })
}

// ---------------------------------------------------------------- builds

fn parse_build(w: &World, b: &J, level: Option<u32>) -> Result<Build, String> {
    let mut b = b.clone();
    if let Some(o) = b.as_object_mut() {
        o.remove("file");
        o.remove("note");
        o.retain(|_, v| !v.is_null());
        if let Some(t) = o.get_mut("talents").and_then(|t| t.as_object_mut()) {
            t.retain(|_, v| v.as_u64().map_or(false, |x| x > 0));
        }
    }
    let table: toml::Table = serde_json::from_value(b).map_err(|e| format!("build: {e}"))?;
    build_from_table(w, &table, "build", level)
}

fn default_opts(w: &World) -> Vec<f64> {
    w.opts.iter().map(|o| o.default).collect()
}

fn opts_for(w: &World, b: &Build, sc: Option<&ScenarioDef>, over: Option<&J>) -> Result<Vec<f64>, String> {
    let mut v = default_opts(w);
    if let Some(sc) = sc {
        for &(i, x) in &sc.opts {
            v[i as usize] = x;
        }
    }
    for &(i, x) in &b.opts {
        v[i as usize] = x;
    }
    if let Some(o) = over.and_then(|o| o.as_object()) {
        for (k, x) in o {
            let i = w.opt_index(k).ok_or_else(|| format!("unknown option '{k}'"))?;
            v[i as usize] = x.as_f64().ok_or_else(|| format!("option {k} must be a number"))?;
        }
    }
    Ok(v)
}

fn describe(w: &World, b: &Build) -> J {
    json!({
        "name": b.name, "class": w.classes[b.class].id, "class_name": w.classes[b.class].name,
        "race": w.races[b.race].id, "race_name": w.races[b.race].name, "level": b.level,
        "split": b.tree_split(w), "gear": b.gear_id, "rotation": b.rotation, "warnings": b.warnings,
    })
}

fn sheet(w: &World, v: &J) -> Result<J, String> {
    let level = v.get("level").and_then(|x| x.as_u64()).map(|x| x as u32);
    let b = parse_build(w, v.get("build").ok_or("sheet needs a build")?, level)?;
    let opts = opts_for(w, &b, None, v.get("opts"))?;
    let mut sim = Sim::new(w, 1, opts);
    let p = sim.spawn_player(&b, 0, 0);
    let before = sim.sheet(p);
    sim.precombat(p);
    let after = sim.sheet(p);
    let rows: Vec<J> = before.iter().zip(after.iter()).map(|((k, a), (_, c))| json!({ "stat": k, "base": a, "buffed": c })).collect();
    let auras: Vec<String> = sim.actors[p].auras.iter().map(|a| w.auras[a.def as usize].name.clone()).collect();
    let modelled = modelled_talents(w, b.class);
    let c = &w.classes[b.class];
    let unmodelled: Vec<String> = c.talents.iter().enumerate().filter(|(i, _)| b.talents[*i] > 0 && !modelled[*i]).map(|(_, t)| t.name.clone()).collect();
    let talents: Map<String, J> = c.talents.iter().enumerate().filter(|(i, _)| b.talents[*i] > 0).map(|(i, t)| (t.id.clone(), J::from(b.talents[i]))).collect();
    Ok(json!({ "build": describe(w, &b), "rows": rows, "auras": auras, "unmodelled": unmodelled, "talents": talents, "gear": gear_json(w, &b) }))
}

/// What the build's gear kit gives at its level: stat totals and weapons.
fn gear_json(w: &World, b: &Build) -> J {
    let g = &b.gear;
    let mut items: Vec<J> = Vec::new();
    let mut add = |label: &str, value: f64, unit: &str| {
        if value.abs() > 1e-9 {
            items.push(json!({ "label": label, "value": (value * 10.0).round() / 10.0, "unit": unit }));
        }
    };
    let sum = |s: Stat| g.stats.iter().filter(|x| x.0 == s).map(|x| x.1).sum::<f64>();
    for (s, label, unit) in [
        (Stat::Str, "Strength", ""), (Stat::Agi, "Agility", ""), (Stat::Sta, "Stamina", ""), (Stat::Int, "Intellect", ""),
        (Stat::Spi, "Spirit", ""), (Stat::Ap, "Attack power", ""), (Stat::Sp, "Spell power", ""), (Stat::HealPower, "Healing power", ""),
        (Stat::MeleeCrit, "Critical strike", "%"), (Stat::Hit, "Hit", "%"), (Stat::Mp5, "Mana every 5 sec", ""),
        (Stat::Armor, "Armor", ""), (Stat::Defense, "Defense", ""), (Stat::Dodge, "Dodge", "%"), (Stat::BlockValue, "Block value", ""),
    ] {
        add(label, sum(s), unit);
    }
    // crit from gear lands on melee, ranged and spell crit alike; show any that differ
    for (s, label) in [(Stat::SpellCrit, "Spell critical strike"), (Stat::RangedCrit, "Ranged critical strike")] {
        if (sum(s) - sum(Stat::MeleeCrit)).abs() > 1e-9 {
            add(label, sum(s), "%");
        }
    }
    let weapon = |slot: &str, wp: &Weapon| {
        json!({
            "slot": slot, "type": wp.wtype.name(), "min": wp.min.round(), "max": wp.max.round(), "speed": wp.speed,
            "dps": (wp.dps() * 10.0).round() / 10.0, "school": wp.school.name(),
        })
    };
    let mut weapons: Vec<J> = Vec::new();
    if let Some(wp) = &g.mh {
        weapons.push(weapon("main_hand", wp));
    }
    if let Some(wp) = &g.oh {
        weapons.push(weapon("off_hand", wp));
    }
    if let Some(wp) = &g.ranged {
        weapons.push(weapon("ranged", wp));
    }
    let shield = g.shield.map(|(armor, block)| json!({ "armor": armor.round(), "block_value": block.round() }));
    let kit = w.gear.get(&b.gear_id);
    json!({ "kit": b.gear_id, "desc": kit.map_or("", |k| k.desc.as_str()), "level": b.level, "stats": items, "weapons": weapons, "shield": shield })
}

// ---------------------------------------------------------------- trace

/// One fight, event by event, for the website's timeline: the run's first
/// replication (the same dice as its first fight). Boss, duel and ambush
/// fights give the first `until` seconds (30 by default); leveling gives the
/// first pull and the rest after it.
fn trace(w: &World, v: &J) -> Result<J, String> {
    let scenario = v.get("scenario").and_then(|x| x.as_str()).ok_or("trace needs a scenario")?;
    let sc = w.scenarios.get(scenario).ok_or_else(|| format!("unknown scenario '{scenario}'"))?;
    let level = v.get("level").and_then(|x| x.as_u64()).map(|x| x as u32);
    let b = parse_build(w, v.get("build").ok_or("trace needs a build")?, level)?;
    let opponent = match v.get("opponent") {
        Some(o) if !o.is_null() => Some(parse_build(w, o, level).map_err(|e| format!("opponent: {e}"))?),
        _ => None,
    };
    if matches!(sc.kind, ScenarioKind::Duel { .. }) && opponent.is_none() {
        return Err("a duel needs an opponent".into());
    }
    let opts = opts_for(w, &b, Some(sc), v.get("opts"))?;
    let cfg = McCfg { seed: v.get("seed").and_then(|x| x.as_u64()).unwrap_or(McCfg::default().seed), ..McCfg::default() };
    let rep = v.get("rep").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
    // leveling: the first pulls play out the same however long the session is
    let mut short = sc.clone();
    if let ScenarioKind::Leveling { minutes, .. } = &mut short.kind {
        *minutes = minutes.min(4.0);
    }
    let (_, _, events) = run_rep_traced(&RepInput { w, sc: &short, build: &b, opponent: opponent.as_ref(), opts: &opts, seed: mc::rep_seed(&cfg, sc, rep), log: true });
    let mut events = events.unwrap_or_default();
    let leveling = matches!(sc.kind, ScenarioKind::Leveling { .. });
    if leveling {
        // from the first pull to the next one (at most two and a half minutes)
        let first = events.iter().position(|e| e.kind == "pull").unwrap_or(0);
        let t0 = events.get(first).map_or(0.0, |e| e.t);
        let next = events.iter().skip(first + 1).position(|e| e.kind == "pull").map(|k| k + first + 1).unwrap_or(events.len());
        events = events.drain(first..next).filter(|e| e.t <= t0 + 150.0).collect();
    } else {
        let until = v.get("until").and_then(|x| x.as_f64()).unwrap_or(30.0);
        events.retain(|e| e.t <= until + 1e-9);
    }
    // what concerns the build: its own (and its pet's) actions, what it puts
    // on others, and what is done to it
    let me = events.iter().find(|e| e.team == 0 && !e.pet).map(|e| e.who.clone()).unwrap_or_else(|| b.name.clone());
    events.retain(|e| e.team == 0 || e.target.as_deref() == Some(me.as_str()) || e.kind == "death");
    Ok(json!({ "me": me, "leveling": leveling, "events": events }))
}

// ---------------------------------------------------------------- sessions

fn start(st: &mut State, world: Arc<World>, v: &J) -> Result<J, String> {
    let w = &*world;
    let scenario = v.get("scenario").and_then(|x| x.as_str()).ok_or("start needs a scenario")?.to_string();
    let sc = w.scenarios.get(&scenario).ok_or_else(|| format!("unknown scenario '{scenario}'"))?;
    let level = v.get("level").and_then(|x| x.as_u64()).map(|x| x as u32);
    let builds = v.get("builds").and_then(|x| x.as_array()).ok_or("start needs builds")?;
    if builds.is_empty() {
        return Err("give at least one build".into());
    }
    let opponent = match v.get("opponent") {
        Some(o) if !o.is_null() => Some(parse_build(w, o, level).map_err(|e| format!("opponent: {e}"))?),
        _ => None,
    };
    if matches!(sc.kind, ScenarioKind::Duel { .. }) && opponent.is_none() {
        return Err("a duel needs an opponent".into());
    }
    let mut variants = Vec::new();
    for (i, bj) in builds.iter().enumerate() {
        let b = parse_build(w, bj, level).map_err(|e| format!("build {}: {e}", i + 1))?;
        let opts = opts_for(w, &b, Some(sc), v.get("opts"))?;
        variants.push(Variant { world: 0, name: b.name.clone(), build: b, opponent: opponent.clone(), opts });
    }
    let d = McCfg::default();
    let cfg = McCfg {
        seed: v.get("seed").and_then(|x| x.as_u64()).unwrap_or(d.seed),
        min_reps: v.get("min_reps").and_then(|x| x.as_u64()).map(|x| x as usize).unwrap_or(32),
        max_reps: v.get("max_reps").and_then(|x| x.as_u64()).map(|x| x as usize).unwrap_or(2000),
        batch: 16,
        precision: v.get("precision").and_then(|x| x.as_f64()).unwrap_or(0.02),
        conf: 0.95,
        threads: 1,
    };
    let id = st.next_id;
    st.next_id += 1;
    let n = variants.len();
    let max_reps = cfg.max_reps;
    st.sessions.insert(
        id,
        Session {
            world,
            scenario,
            variants,
            values: vec![Vec::new(); n],
            pending: BTreeMap::new(),
            checked: 0,
            precision: vec![f64::INFINITY; n],
            cfg,
            converged: false,
            finished: false,
        },
    );
    Ok(json!({ "id": id, "max_reps": max_reps }))
}

fn session<'a>(st: &'a mut State, v: &J) -> Result<&'a mut Session, String> {
    let id = v.get("id").and_then(|x| x.as_u64()).ok_or("missing session id")?;
    st.sessions.get_mut(&id).ok_or_else(|| "unknown or expired session".to_string())
}

fn step(st: &mut State, v: &J) -> Result<J, String> {
    let s = session(st, v)?;
    let want = v.get("reps").and_then(|x| x.as_u64()).unwrap_or(8).max(1) as usize;
    if !s.finished {
        let from = s.values[0].len();
        let chunk = run_range(s, from, want)?;
        absorb(s, from, chunk)?;
    }
    Ok(progress(s))
}

/// Replications `from..from+count` (capped at `max_reps`) for every variant.
fn run_range(s: &Session, from: usize, count: usize) -> Result<Chunk, String> {
    let w = &*s.world;
    let sc = w.scenarios.get(&s.scenario).ok_or("scenario vanished")?;
    let to = from.saturating_add(count).min(s.cfg.max_reps);
    let mut chunk: Chunk = vec![Vec::new(); s.variants.len()];
    for r in from..to.max(from) {
        let seed = mc::rep_seed(&s.cfg, sc, r);
        for (vi, var) in s.variants.iter().enumerate() {
            let (vals, _) = run_rep(&RepInput { w, sc, build: &var.build, opponent: var.opponent.as_ref(), opts: &var.opts, seed, log: false });
            chunk[vi].push(vals);
        }
    }
    Ok(chunk)
}

/// Store a chunk starting at replication `from`, then check convergence at
/// every checkpoint the in-order results have reached.
fn absorb(s: &mut Session, from: usize, chunk: Chunk) -> Result<(), String> {
    if chunk.len() != s.variants.len() || chunk.iter().any(|c| c.len() != chunk[0].len()) {
        return Err("chunk does not match the session".into());
    }
    if s.finished || from < s.values[0].len() {
        return Ok(());
    }
    s.pending.insert(from, chunk);
    while let Some(chunk) = s.pending.remove(&s.values[0].len()) {
        for (vals, more) in s.values.iter_mut().zip(chunk) {
            vals.extend(more);
        }
    }
    let w = &*s.world;
    let sc = w.scenarios.get(&s.scenario).ok_or("scenario vanished")?;
    let every = s.cfg.batch.max(1);
    let have = s.values[0].len();
    while !s.finished {
        let at = ((s.checked / every + 1) * every).max(s.cfg.min_reps).min(s.cfg.max_reps);
        if at > have || at <= s.checked {
            s.finished = at <= s.checked;
            break;
        }
        s.checked = at;
        s.precision = s.values.iter().map(|vals| mc::precision_of(&vals[..at], &sc.primary, s.cfg.conf)).collect();
        if at >= s.cfg.min_reps && s.precision.iter().all(|&p| p <= s.cfg.precision) {
            s.converged = true;
            s.finished = true;
            s.values.iter_mut().for_each(|vals| vals.truncate(at));
        } else if at >= s.cfg.max_reps {
            s.finished = true;
        }
    }
    if s.finished {
        s.pending.clear();
    }
    Ok(())
}

fn progress(s: &Session) -> J {
    let precision: Vec<J> = s.precision.iter().map(|&p| if p.is_finite() { J::from(p) } else { J::Null }).collect();
    json!({
        "reps": s.values[0].len(), "max_reps": s.cfg.max_reps, "target": s.cfg.precision,
        "precision": precision, "converged": s.converged, "finished": s.finished,
    })
}

fn run_chunk(st: &mut State, v: &J) -> Result<J, String> {
    let s = session(st, v)?;
    let from = v.get("from").and_then(|x| x.as_u64()).ok_or("run needs from")? as usize;
    let count = v.get("count").and_then(|x| x.as_u64()).ok_or("run needs count")? as usize;
    let chunk = run_range(s, from, count)?;
    Ok(json!({ "from": from, "values": chunk.iter().map(|reps| encode_reps(reps)).collect::<Vec<_>>() }))
}

fn add(st: &mut State, v: &J) -> Result<J, String> {
    let s = session(st, v)?;
    let c = v.get("chunk").ok_or("add needs a chunk")?;
    if let Some(e) = c.get("error") {
        return Err(format!("a worker failed: {e}"));
    }
    let from = c.get("from").and_then(|x| x.as_u64()).ok_or("chunk without from")? as usize;
    let chunk = c.get("values").and_then(|x| x.as_array()).ok_or("chunk without values")?.iter().map(decode_reps).collect::<Result<Chunk, _>>()?;
    absorb(s, from, chunk)?;
    Ok(progress(s))
}

/// Replications as {k: [metric names], r: [[value per name]]}: a missing
/// metric is null, a non-finite value a string, so decoding gives back
/// exactly the same maps (serde_json's float_roundtrip keeps every bit).
fn encode_reps(reps: &[Values]) -> J {
    let keys: Vec<&String> = reps.iter().flat_map(|v| v.keys()).collect::<BTreeSet<_>>().into_iter().collect();
    let rows: Vec<J> = reps
        .iter()
        .map(|v| {
            J::Array(
                keys.iter()
                    .map(|k| match v.get(*k) {
                        None => J::Null,
                        Some(x) if x.is_finite() => J::from(*x),
                        Some(x) => J::String(x.to_string()),
                    })
                    .collect(),
            )
        })
        .collect();
    json!({ "k": keys, "r": rows })
}

fn decode_reps(j: &J) -> Result<Vec<Values>, String> {
    let bad = || "malformed chunk".to_string();
    let keys: Vec<&str> = j.get("k").and_then(|x| x.as_array()).ok_or_else(bad)?.iter().map(|k| k.as_str().ok_or_else(bad)).collect::<Result<_, _>>()?;
    let rows = j.get("r").and_then(|x| x.as_array()).ok_or_else(bad)?;
    rows.iter()
        .map(|row| {
            let row = row.as_array().filter(|r| r.len() == keys.len()).ok_or_else(bad)?;
            let mut out = Values::new();
            for (k, x) in keys.iter().zip(row) {
                let x = match x {
                    J::Null => continue,
                    J::String(t) => t.parse::<f64>().map_err(|_| bad())?,
                    x => x.as_f64().ok_or_else(bad)?,
                };
                out.insert(k.to_string(), x);
            }
            Ok(out)
        })
        .collect()
}

fn result(st: &mut State, v: &J) -> Result<J, String> {
    let s = session(st, v)?;
    let w = &*s.world;
    let sc = w.scenarios.get(&s.scenario).ok_or("scenario vanished")?;
    let reps = s.values[0].len();
    if reps == 0 {
        return Err("no replications yet".into());
    }
    let res = McResult { reps, converged: s.converged, values: s.values.clone() };
    let columns: Vec<&str> = v.get("columns").and_then(|c| c.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    let mut variants = Vec::new();
    for (vi, var) in s.variants.iter().enumerate() {
        let sums = mc::summarise(&res.values[vi], s.cfg.conf);
        let mut out = json!({
            "build": describe(w, &var.build),
            "opponent": var.opponent.as_ref().map(|o| describe(w, o)),
            "metrics": sums,
        });
        if !columns.is_empty() {
            let cols: Map<String, J> = columns.iter().map(|m| (m.to_string(), J::from(mc::column(&res.values[vi], m)))).collect();
            out["columns"] = J::Object(cols);
        }
        variants.push(out);
    }
    let names: Vec<String> = s.variants.iter().map(|v| v.name.clone()).collect();
    let mut metrics = vec![sc.primary.clone()];
    for m in mc::summarise(&res.values[0], s.cfg.conf).iter().filter(|m| !m.name.contains(':') && m.name != sc.primary) {
        metrics.push(m.name.clone());
    }
    let n = names.len();
    let pairs: Vec<(usize, usize)> = match v.get("pairs") {
        None | Some(J::Null) => (1..n).map(|b| (0, b)).collect(),
        Some(J::String(all)) if all == "all" => (0..n).flat_map(|a| (0..n).filter(move |&b| b != a).map(move |b| (a, b))).collect(),
        Some(p) => p
            .as_array()
            .ok_or("pairs must be a list of [a, b]")?
            .iter()
            .map(|x| match x.as_array().map(|ab| ab.iter().map(|i| i.as_u64()).collect::<Vec<_>>()).as_deref() {
                Some([Some(a), Some(b)]) if a != b && (*a as usize) < names.len() && (*b as usize) < names.len() => Ok((*a as usize, *b as usize)),
                _ => Err(format!("bad pair {x}: give two different build indices below {}", names.len())),
            })
            .collect::<Result<_, _>>()?,
    };
    let paired = serde_json::to_value(mc::compare_pairs(&names, &res, &metrics, &pairs, s.cfg.conf)).unwrap_or(J::Null);
    Ok(json!({
        "scenario": { "id": sc.id, "desc": sc.desc, "primary": sc.primary },
        "reps": reps, "converged": s.converged, "seed": s.cfg.seed, "data_build": w.meta.data_build,
        "variants": variants, "paired": paired,
    }))
}

fn pair(v: &J) -> Result<J, String> {
    let metric = v.get("metric").and_then(|x| x.as_str()).ok_or("pair needs a metric")?;
    let read = |k: &str| -> Result<Vec<f64>, String> {
        let a = v.get(k).and_then(|x| x.as_array()).ok_or_else(|| format!("pair needs {k}: a list of values"))?;
        // non-finite values come back from `columns` as null
        a.iter().map(|x| if x.is_null() { Ok(f64::NAN) } else { x.as_f64().ok_or_else(|| format!("{k} holds something that is not a number")) }).collect()
    };
    let (a, b) = (read("a")?, read("b")?);
    let n = a.len().min(b.len());
    if n < 2 {
        return Err("pair needs at least two replications in common".into());
    }
    let conf = v.get("conf").and_then(|x| x.as_f64()).unwrap_or(0.95);
    let (diff, lo, hi, vr) = mc::pair_stats(metric, &a[..n], &b[..n], conf);
    let num = |x: f64| if x.is_finite() { J::from(x) } else { J::Null };
    Ok(json!({
        "metric": metric, "n": n, "diff": num(diff), "lo": num(lo), "hi": num(hi),
        "significant": lo > 0.0 || hi < 0.0, "variance_reduction": num(vr),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(j: J) -> J {
        serde_json::from_str(&handle(&j.to_string())).unwrap()
    }

    #[test]
    fn catalog_lists_everything() {
        let c = call(json!({ "op": "catalog" }));
        assert_eq!(c["classes"].as_array().unwrap().len(), 9);
        assert!(c["presets"].as_array().unwrap().len() >= 9);
        assert!(c["scenarios"].as_array().unwrap().iter().any(|s| s["id"] == "boss"));
    }

    #[test]
    fn a_session_runs_to_a_result() {
        let c = call(json!({ "op": "catalog" }));
        let preset = c["presets"].as_array().unwrap().iter().find(|p| p["file"] == "warrior/vampire_v2.toml").unwrap().clone();
        let s = call(json!({ "op": "sheet", "build": preset }));
        assert!(s.get("error").is_none(), "{s}");
        let id = call(json!({ "op": "start", "scenario": "boss", "builds": [preset.clone(), preset], "precision": 0.05 }))["id"].as_u64().unwrap();
        let mut finished = false;
        for _ in 0..200 {
            let p = call(json!({ "op": "step", "id": id, "reps": 16 }));
            assert!(p.get("error").is_none(), "{p}");
            if p["finished"].as_bool().unwrap() {
                finished = true;
                break;
            }
        }
        assert!(finished);
        let r = call(json!({ "op": "result", "id": id }));
        assert_eq!(r["variants"].as_array().unwrap().len(), 2);
        // identical builds on common random numbers: the paired difference is exactly zero
        assert_eq!(r["paired"][0]["diff"].as_f64().unwrap(), 0.0);
    }

    /// The page splits a run between Web Workers: chunks of any size, arriving
    /// in any order, must give exactly what one worker stepping through gives.
    #[test]
    fn split_runs_match_one_worker() {
        let c = call(json!({ "op": "catalog" }));
        let pick = |f: &str| c["presets"].as_array().unwrap().iter().find(|p| p["file"] == f).unwrap().clone();
        for (scenario, precision) in [("pressure", 0.04), ("boss", 0.01), ("solo_1", 0.03)] {
            let start = json!({ "op": "start", "scenario": scenario, "builds": [pick("warrior/vampire_v2.toml"), pick("rogue/mutilate.toml")], "precision": precision, "max_reps": 400 });
            let one = call(start.clone())["id"].as_u64().unwrap();
            while !call(json!({ "op": "step", "id": one, "reps": 7 }))["finished"].as_bool().unwrap() {}
            let expect = call(json!({ "op": "result", "id": one }));

            let keeper = call(start.clone())["id"].as_u64().unwrap();
            let worker = call(start)["id"].as_u64().unwrap();
            let sizes = [3usize, 1, 9, 2, 16, 5, 30, 4];
            let (mut from, mut chunks) = (0, Vec::new());
            for i in 0.. {
                if from >= 400 {
                    break;
                }
                let n = sizes[i % sizes.len()];
                chunks.push(call(json!({ "op": "run", "id": worker, "from": from, "count": n })));
                from += n;
            }
            // deliver in a scrambled order: later chunks first within each group of three
            let mut last = J::Null;
            for group in chunks.chunks(3) {
                for ch in group.iter().rev() {
                    last = call(json!({ "op": "add", "id": keeper, "chunk": ch }));
                    assert!(last.get("error").is_none(), "{last}");
                }
            }
            assert!(last["finished"].as_bool().unwrap());
            let got = call(json!({ "op": "result", "id": keeper }));
            assert_eq!(got, expect, "{scenario}");
        }
    }

    #[test]
    fn results_compare_the_pairs_asked_for() {
        let c = call(json!({ "op": "catalog" }));
        let pick = |f: &str| c["presets"].as_array().unwrap().iter().find(|p| p["file"] == f).unwrap().clone();
        let builds = json!([pick("warrior/vampire_v2.toml"), pick("warrior/conventional_fury.toml"), pick("warrior/vampire_v2.toml")]);
        let id = call(json!({ "op": "start", "scenario": "boss", "builds": builds, "precision": 0.05, "max_reps": 64 }))["id"].as_u64().unwrap();
        while !call(json!({ "op": "step", "id": id, "reps": 32 }))["finished"].as_bool().unwrap() {}
        let primary = |r: &J| r["paired"].as_array().unwrap().iter().filter(|p| p["metric"] == "dps").cloned().collect::<Vec<_>>();
        let default = primary(&call(json!({ "op": "result", "id": id })));
        assert_eq!(default.iter().map(|p| (p["a"].as_u64().unwrap(), p["b"].as_u64().unwrap())).collect::<Vec<_>>(), vec![(0, 1), (0, 2)]);
        let asked = primary(&call(json!({ "op": "result", "id": id, "pairs": [[1, 2], [2, 0]] })));
        assert_eq!(asked.iter().map(|p| (p["a"].as_u64().unwrap(), p["b"].as_u64().unwrap())).collect::<Vec<_>>(), vec![(1, 2), (2, 0)]);
        // builds 0 and 2 are identical, so on the same dice their difference is exactly zero
        assert_eq!(asked[1]["diff"].as_f64().unwrap(), 0.0);
        // and 1 -> 2 is the mirror of 0 -> 1
        assert_eq!(asked[0]["diff"].as_f64().unwrap(), -default[0]["diff"].as_f64().unwrap());
        for bad in [json!([[0, 0]]), json!([[0, 3]]), json!([[1]]), json!("some")] {
            assert!(call(json!({ "op": "result", "id": id, "pairs": bad }))["error"].is_string(), "{bad}");
        }
    }

    /// Columns kept from separate runs pair exactly like builds run together,
    /// and every ordered pair is the mirror of its reverse.
    #[test]
    fn stored_columns_pair_like_a_joint_run() {
        let c = call(json!({ "op": "catalog" }));
        let pick = |f: &str| c["presets"].as_array().unwrap().iter().find(|p| p["file"] == f).unwrap().clone();
        let (a, b) = (pick("warrior/vampire_v2.toml"), pick("warrior/conventional_fury.toml"));
        for (scenario, metric) in [("boss", "dps"), ("pressure", "breakeven")] {
            let run = |builds: J| {
                let id = call(json!({ "op": "start", "scenario": scenario, "builds": builds, "precision": 0.0, "min_reps": 48, "max_reps": 48 }))["id"].as_u64().unwrap();
                while !call(json!({ "op": "step", "id": id, "reps": 16 }))["finished"].as_bool().unwrap() {}
                call(json!({ "op": "result", "id": id, "pairs": "all", "columns": [metric] }))
            };
            let joint = run(json!([a.clone(), b.clone()]));
            let alone_a = run(json!([a.clone()]));
            let alone_b = run(json!([b.clone()]));
            let col = |r: &J, i: usize| r["variants"][i]["columns"][metric].clone();
            assert_eq!(col(&alone_a, 0), col(&joint, 0), "{scenario}: same build, same values");
            assert_eq!(col(&alone_b, 0), col(&joint, 1));
            let paired: Vec<J> = joint["paired"].as_array().unwrap().iter().filter(|p| p["metric"] == metric).cloned().collect();
            assert_eq!(paired.len(), 2);
            let ab = paired.iter().find(|p| p["a"] == 0 && p["b"] == 1).unwrap();
            let ba = paired.iter().find(|p| p["a"] == 1 && p["b"] == 0).unwrap();
            assert_eq!(ab["diff"].as_f64().unwrap(), -ba["diff"].as_f64().unwrap());
            let p = call(json!({ "op": "pair", "metric": metric, "a": col(&alone_a, 0), "b": col(&alone_b, 0) }));
            for k in ["diff", "lo", "hi", "significant"] {
                assert_eq!(p[k], ab[k], "{scenario} {k}");
            }
        }
        assert!(call(json!({ "op": "pair", "metric": "dps", "a": [1.0], "b": [2.0] }))["error"].is_string());
        assert!(call(json!({ "op": "pair", "metric": "dps", "a": "x", "b": [2.0] }))["error"].is_string());
    }

    #[test]
    fn bad_input_is_an_error_not_a_crash() {
        assert!(call(json!({ "op": "start", "scenario": "boss", "builds": [{ "class": "warrior" }] }))["error"].is_string());
        assert!(call(json!({ "op": "nope" }))["error"].is_string());
        assert!(serde_json::from_str::<J>(&handle("not json")).unwrap()["error"].is_string());
    }
}
