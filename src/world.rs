//! Loading the data directory into a compiled `World`.
//!
//! Layout:
//!   data/base/*.toml            merged at the root of the data tree
//!   data/base/<dir>/<name>.toml merged under `<dir>.<name>` (e.g. classes.warrior)
//!   data/patches/*.toml         overlays applied in build order (see README)
//!   --patch <file>              extra overlays applied last ("what if" changes)
//!
//! Overlay rules: tables merge key by key, anything else replaces, and a
//! table containing `__delete = true` removes the key.

use crate::expr::Expr;
use crate::model::*;
use crate::vars::*;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use toml::{Table, Value};

type R<T> = Result<T, String>;

#[derive(Clone, Debug, Default)]
pub struct LoadOptions {
    pub data_dir: PathBuf,
    /// Only apply patches whose build is <= this (e.g. "1.60.1.70338").
    pub at_build: Option<String>,
    pub extra_patches: Vec<PathBuf>,
    pub skip_patches: bool,
}

/// "improved_heroic_strike" -> "Improved Heroic Strike", "natures_grace" -> "Nature's Grace".
/// Used when a talent has no explicit `name`.
pub fn pretty_name(id: &str) -> String {
    const POSSESSIVE: &[(&str, &str)] = &[
        ("natures", "Nature's"), ("predators", "Predator's"), ("survivalists", "Survivalist's"), ("lights", "Light's"),
        ("templars", "Templar's"), ("guardians", "Guardian's"), ("earths", "Earth's"), ("winters", "Winter's"),
        ("hunters", "Hunter's"), ("elunes", "Elune's"),
    ];
    const SMALL: &[&str] = &["of", "the", "and", "an", "a", "for", "to", "in", "on"];
    id.split('_')
        .enumerate()
        .map(|(i, w)| {
            if let Some((_, p)) = POSSESSIVE.iter().find(|(k, _)| *k == w) {
                return p.to_string();
            }
            if i > 0 && SMALL.contains(&w) {
                return w.to_string();
            }
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_toml(name: &str, text: &str) -> R<Value> {
    let t: Table = toml::from_str(text).map_err(|e| format!("{name}: {e}"))?;
    Ok(Value::Table(t))
}

pub fn merge(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Table(b), Value::Table(p)) => {
            for (k, v) in p {
                let del = matches!(&v, Value::Table(t) if t.get("__delete").and_then(|x| x.as_bool()) == Some(true));
                if del {
                    b.remove(&k);
                } else if let Some(existing) = b.get_mut(&k) {
                    merge(existing, v);
                } else {
                    b.insert(k, v);
                }
            }
        }
        (b, p) => *b = p,
    }
}

fn sorted_toml_files(dir: &Path) -> R<Vec<PathBuf>> {
    let mut v = Vec::new();
    if !dir.exists() {
        return Ok(v);
    }
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.extension().and_then(|x| x.to_str()) == Some("toml") {
            v.push(p);
        }
    }
    v.sort();
    Ok(v)
}

fn build_key(s: &str) -> Vec<u64> {
    s.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).map(|x| x.parse().unwrap_or(0)).collect()
}

/// Data files held in memory, with paths relative to the data directory:
/// `base/core.toml`, `base/classes/warrior.toml`, `patches/1.60.2.toml`.
/// This is what the web version loads (the files are embedded at build time).
pub struct MemData<'a> {
    pub files: &'a [(String, String)],
    pub at_build: Option<&'a str>,
    /// Extra overlays applied last: (name, toml text).
    pub extra: &'a [(String, String)],
    pub skip_patches: bool,
}

/// Merge base + patches (+ extra overlays) into one tree.
pub fn load_tree_mem(d: &MemData) -> R<(Value, WorldMeta)> {
    let mut root_files: Vec<&(String, String)> = Vec::new();
    let mut sub_files: Vec<(&str, &str, &(String, String))> = Vec::new();
    let mut patch_files: Vec<&(String, String)> = Vec::new();
    for f in d.files {
        let p = f.0.replace('\\', "/");
        if !p.ends_with(".toml") {
            continue;
        }
        if let Some(rest) = p.strip_prefix("base/") {
            match rest.split_once('/') {
                None => root_files.push(f),
                Some((dir, file)) if !file.contains('/') => {
                    let dir = &f.0[5..5 + dir.len()];
                    let stem = &f.0[5 + dir.len() + 1..f.0.len() - 5];
                    sub_files.push((dir, stem, f));
                }
                _ => {}
            }
        } else if let Some(rest) = p.strip_prefix("patches/") {
            if !rest.contains('/') {
                patch_files.push(f);
            }
        }
    }
    root_files.sort_by(|a, b| a.0.cmp(&b.0));
    sub_files.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    let mut root = Value::Table(Table::new());
    for f in root_files {
        merge(&mut root, parse_toml(&f.0, &f.1)?);
    }
    for (dir, stem, f) in sub_files {
        let mut inner = Table::new();
        inner.insert(stem.to_string(), parse_toml(&f.0, &f.1)?);
        let mut outer = Table::new();
        outer.insert(dir.to_string(), Value::Table(inner));
        merge(&mut root, Value::Table(outer));
    }
    let mut meta = WorldMeta::default();
    if let Some(b) = root.get("meta").and_then(|m| m.get("data_build")).and_then(|v| v.as_str()) {
        meta.data_build = b.to_string();
    }
    if !d.skip_patches {
        let mut patches: Vec<(Vec<u64>, String, Value)> = Vec::new();
        for f in patch_files {
            let v = parse_toml(&f.0, &f.1)?;
            let build = v.get("meta").and_then(|m| m.get("build")).and_then(|b| b.as_str()).unwrap_or("").to_string();
            patches.push((build_key(&build), f.0.clone(), v));
        }
        patches.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let cutoff = d.at_build.map(build_key);
        for (key, name, mut v) in patches {
            if let Some(c) = &cutoff {
                if !key.is_empty() && &key > c {
                    continue;
                }
            }
            if let Value::Table(t) = &mut v {
                if let Some(b) = t.get("meta").and_then(|m| m.get("build")).and_then(|b| b.as_str()) {
                    meta.data_build = b.to_string();
                }
                t.remove("meta");
            }
            merge(&mut root, v);
            meta.patches.push(name.rsplit('/').next().unwrap_or(&name).to_string());
        }
    }
    for (name, text) in d.extra {
        let mut v = parse_toml(name, text)?;
        if let Value::Table(t) = &mut v {
            t.remove("meta");
        }
        merge(&mut root, v);
        meta.patches.push(format!("{name} (extra)"));
    }
    Ok((root, meta))
}

/// Read every data file under a directory into memory (paths relative to it).
pub fn read_data_dir(dir: &Path) -> R<Vec<(String, String)>> {
    let mut out = Vec::new();
    for sub in ["base", "patches"] {
        let d = dir.join(sub);
        for f in sorted_toml_files(&d)? {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            out.push((format!("{sub}/{}", f.file_name().unwrap().to_string_lossy()), text));
        }
        if sub == "base" && d.exists() {
            let mut subdirs: Vec<PathBuf> = std::fs::read_dir(&d)
                .map_err(|e| format!("{}: {e}", d.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir())
                .collect();
            subdirs.sort();
            for sd in subdirs {
                let dname = sd.file_name().unwrap().to_string_lossy().to_string();
                for f in sorted_toml_files(&sd)? {
                    let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
                    out.push((format!("base/{dname}/{}", f.file_name().unwrap().to_string_lossy()), text));
                }
            }
        }
    }
    if !dir.join("base").exists() {
        return Err(format!("{}: no base/ folder (is --data pointing at the data directory?)", dir.display()));
    }
    Ok(out)
}

/// Read base + patches from disk into one merged tree.
pub fn load_tree(o: &LoadOptions) -> R<(Value, WorldMeta)> {
    let files = read_data_dir(&o.data_dir)?;
    let mut extra = Vec::new();
    for f in &o.extra_patches {
        let text = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
        extra.push((f.display().to_string(), text));
    }
    load_tree_mem(&MemData { files: &files, at_build: o.at_build.as_deref(), extra: &extra, skip_patches: o.skip_patches })
}

pub fn load_world_mem(d: &MemData) -> R<World> {
    let (tree, meta) = load_tree_mem(d)?;
    compile(tree, meta)
}

pub fn load_world(o: &LoadOptions) -> R<World> {
    let (tree, meta) = load_tree(o)?;
    compile(tree, meta)
}

// ---------------------------------------------------------------- helpers

fn tbl<'a>(v: &'a Value, path: &str) -> R<&'a Table> {
    v.as_table().ok_or_else(|| format!("{path}: expected a table"))
}
fn sub<'a>(t: &'a Table, k: &str) -> Option<&'a Table> {
    t.get(k).and_then(|v| v.as_table())
}
fn s(t: &Table, k: &str) -> Option<String> {
    t.get(k).and_then(|v| v.as_str()).map(|x| x.to_string())
}
fn f(t: &Table, k: &str, d: f64) -> f64 {
    match t.get(k) {
        Some(Value::Integer(i)) => *i as f64,
        Some(Value::Float(x)) => *x,
        Some(Value::Boolean(b)) => {
            if *b { 1.0 } else { 0.0 }
        }
        _ => d,
    }
}
fn bl(t: &Table, k: &str, d: bool) -> bool {
    t.get(k).and_then(|v| v.as_bool()).unwrap_or(d)
}
fn src_of(v: &Value) -> Option<String> {
    match v {
        Value::Integer(i) => Some(i.to_string()),
        Value::Float(x) => Some(format!("{x:?}")),
        Value::Boolean(b) => Some(if *b { "1".into() } else { "0".into() }),
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}
fn strs(t: &Table, k: &str) -> Vec<String> {
    match t.get(k) {
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => vec![],
    }
}
fn num_list(v: Option<&Value>) -> Vec<f64> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| match x {
                Value::Integer(i) => Some(*i as f64),
                Value::Float(f) => Some(*f),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}
fn entries(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(t @ Value::Table(_)) => vec![t],
        _ => vec![],
    }
}

#[derive(Clone, Copy)]
struct Ctx {
    class: Option<usize>,
    rank_alias: Option<u16>,
}

struct Reg {
    w: World,
    opt_idx: HashMap<String, u16>,
    form_idx: HashMap<String, u16>,
    pet_idx: HashMap<String, u16>,
    proc_key: u32,
}

impl Reg {
    fn intern(list: &mut Vec<String>, s: &str) -> u16 {
        if let Some(i) = list.iter().position(|x| x == s) {
            return i as u16;
        }
        list.push(s.to_string());
        (list.len() - 1) as u16
    }

    fn resolve(&mut self, name: &str, ctx: Ctx) -> R<VarRef> {
        let w = &self.w;
        let (who, rest) = if let Some(r) = name.strip_prefix("target.") {
            (Who::Target, r)
        } else if let Some(r) = name.strip_prefix("owner.") {
            (Who::Owner, r)
        } else if let Some(r) = name.strip_prefix("self.") {
            (Who::Me, r)
        } else if let Some(r) = name.strip_prefix("pet.") {
            (Who::Pet, r)
        } else {
            (Who::Me, name)
        };
        if who == Who::Me {
            if let Some(t) = rest.strip_prefix("t.") {
                let c = ctx.class.ok_or_else(|| format!("'{name}': talents can only be used inside a class file"))?;
                let i = w.classes[c].talent_index.get(t).ok_or_else(|| {
                    format!("'{name}': no talent '{t}' in class {}", w.classes[c].id)
                })?;
                return Ok(VarRef::Talent(*i));
            }
            if let Some(r) = rest.strip_prefix("r.") {
                return Ok(VarRef::Rank(Self::intern(&mut self.w.rank_fields, r)));
            }
            if let Some(o) = rest.strip_prefix("opt.") {
                return self
                    .opt_idx
                    .get(o)
                    .map(|&i| VarRef::Opt(i))
                    .ok_or_else(|| format!("'{name}': unknown option '{o}' (declare it under [options])"));
            }
            if let Some(e) = rest.strip_prefix("event.") {
                if let Some(p) = e.strip_prefix("pet.") {
                    return self.pet_idx.get(p).map(|&i| VarRef::EventPet(i)).ok_or_else(|| format!("'{name}': unknown pet"));
                }
                return EvField::from_name(e).map(VarRef::Event).ok_or_else(|| format!("'{name}': unknown event field"));
            }
            if let Some(c) = rest.strip_prefix("cd.") {
                let (sp, field) = c.rsplit_once('.').unwrap_or((c, "ready"));
                let id = w.find_spell(ctx.class, sp).ok_or_else(|| format!("'{name}': unknown spell '{sp}'"))?;
                return match field {
                    "ready" => Ok(VarRef::CdReady(id)),
                    "remains" => Ok(VarRef::CdRemains(id)),
                    _ => Err(format!("'{name}': use cd.<spell>.ready or .remains")),
                };
            }
            if let Some(sp) = rest.strip_prefix("spell.") {
                let id = w.find_spell(ctx.class, sp).ok_or_else(|| format!("'{name}': unknown spell '{sp}'"))?;
                return Ok(VarRef::Spell(id as u16));
            }
            if let Some(sp) = rest.strip_prefix("known.") {
                let id = w.find_spell(ctx.class, sp).ok_or_else(|| format!("'{name}': unknown spell '{sp}'"))?;
                return Ok(VarRef::Known(id));
            }
            if let Some(c) = rest.strip_prefix("class.") {
                // A class a race lists but with no data file yet is simply never true.
                let i = match w.class_index(c) {
                    Some(i) => i as u16,
                    None if w.races.iter().any(|r| r.classes.iter().any(|x| x == c)) => u16::MAX,
                    None => return Err(format!("'{name}': unknown class")),
                };
                return Ok(VarRef::Class(i));
            }
            if let Some(r) = rest.strip_prefix("race.") {
                let i = w.race_index(r).ok_or_else(|| format!("'{name}': unknown race"))?;
                return Ok(VarRef::Race(i as u16));
            }
            if let Some(fm) = rest.strip_prefix("form.") {
                if fm == "any" {
                    return Ok(VarRef::InAnyForm);
                }
                return self.form_idx.get(fm).map(|&i| VarRef::Form(i)).ok_or_else(|| format!("'{name}': unknown form"));
            }
            for (pre, hand) in [("mh.", Hand::Mh), ("oh.", Hand::Oh), ("ranged.", Hand::Ranged)] {
                if let Some(x) = rest.strip_prefix(pre) {
                    if let Some(fl) = WField::from_name(x) {
                        return Ok(VarRef::Weapon(hand, fl));
                    }
                    if let Some(wt) = WeaponType::from_name(x) {
                        return Ok(VarRef::WeaponFamily(hand, wt as u16));
                    }
                    return Err(format!("'{name}': unknown weapon field"));
                }
            }
            match rest {
                "rank" => {
                    return ctx.rank_alias.map(VarRef::Talent).ok_or_else(|| format!("'rank' is only available inside a talent"))
                }
                "stacks" => return Ok(VarRef::Stacks),
                "cp" | "combo" => return Ok(VarRef::Combo),
                "time" => return Ok(VarRef::Time),
                "combat_time" => return Ok(VarRef::CombatTime),
                "behind" => return Ok(VarRef::Behind),
                "enemies" => return Ok(VarRef::Enemies),
                "in_combat" => return Ok(VarRef::InCombat),
                "gcd_remains" => return Ok(VarRef::GcdRemains),
                "dual_wield" => return Ok(VarRef::DualWield),
                "two_hand" => return Ok(VarRef::TwoHand),
                "has_shield" => return Ok(VarRef::HasShield),
                "in_form" => return Ok(VarRef::InAnyForm),
                _ => {}
            }
        }
        if who == Who::Pet {
            if rest == "active" {
                return Ok(VarRef::PetActive(None));
            }
            let pid = rest.strip_suffix(".active").unwrap_or(rest);
            if let Some(&i) = self.pet_idx.get(pid) {
                return Ok(VarRef::PetActive(Some(i)));
            }
        }
        if let Some(a) = rest.strip_prefix("aura.") {
            let (aname, field) = match a.rsplit_once('.') {
                Some((n, fl)) if ["up", "stacks", "remains", "charges"].contains(&fl) => (n, fl),
                _ => (a, "up"),
            };
            let id = w.find_aura(ctx.class, aname).ok_or_else(|| format!("'{name}': unknown aura '{aname}'"))?;
            return Ok(match field {
                "up" => VarRef::AuraUp(who, id),
                "stacks" => VarRef::AuraStacks(who, id),
                "remains" => VarRef::AuraRemains(who, id),
                _ => VarRef::AuraCharges(who, id),
            });
        }
        match rest {
            "level" => return Ok(VarRef::Level(who)),
            "alive" => return Ok(VarRef::Alive(who)),
            "casting" => return Ok(VarRef::Casting(who)),
            "is_player" => return Ok(VarRef::IsPlayer(who)),
            "boss" => return Ok(VarRef::Boss(who)),
            "base_mana" => return Ok(VarRef::BaseMana(who)),
            "base_hp" => return Ok(VarRef::BaseHp(who)),
            "frozen" => return Ok(VarRef::Mech(who, Mechanic::Freeze)),
            "stunned" => return Ok(VarRef::Mech(who, Mechanic::Stun)),
            "feared" => return Ok(VarRef::Mech(who, Mechanic::Fear)),
            "rooted" => return Ok(VarRef::Mech(who, Mechanic::Root)),
            "silenced" => return Ok(VarRef::Mech(who, Mechanic::Silence)),
            "disarmed" => return Ok(VarRef::Mech(who, Mechanic::Disarm)),
            "incapacitated" => return Ok(VarRef::Mech(who, Mechanic::Incap)),
            "slowed" => return Ok(VarRef::Mech(who, Mechanic::Slow)),
            "crit" => return Ok(VarRef::Stat(who, Stat::MeleeCrit)),
            "max_hp" => return Ok(VarRef::ResMax(who, Res::Hp)),
            _ => {}
        }
        if let Some(ct) = CreatureType::from_name(rest) {
            return Ok(VarRef::Creature(who, ct));
        }
        if let Some(r) = Res::from_name(rest) {
            return Ok(VarRef::Res(who, r));
        }
        if let Some(r) = rest.strip_suffix("_pct").and_then(Res::from_name) {
            return Ok(VarRef::ResPct(who, r));
        }
        if let Some(r) = rest.strip_prefix("max_").and_then(Res::from_name) {
            return Ok(VarRef::ResMax(who, r));
        }
        if let Some(st) = Stat::from_name(rest) {
            return Ok(VarRef::Stat(who, st));
        }
        Err(format!("unknown name '{name}'"))
    }

    fn expr(&mut self, src: &str, ctx: Ctx, path: &str) -> R<Expr> {
        let mut resolve = |n: &str| self.resolve(n, ctx);
        Expr::compile(src, &mut resolve).map_err(|e| format!("{path}: {e}"))
    }
    fn ex(&mut self, t: &Table, k: &str, ctx: Ctx, path: &str) -> R<Option<Expr>> {
        match t.get(k) {
            None => Ok(None),
            Some(v) => {
                let src = src_of(v).ok_or_else(|| format!("{path}.{k}: expected a number or formula"))?;
                Ok(Some(self.expr(&src, ctx, &format!("{path}.{k}"))?))
            }
        }
    }
    fn exd(&mut self, t: &Table, k: &str, d: &str, ctx: Ctx, path: &str) -> R<Expr> {
        Ok(match self.ex(t, k, ctx, path)? {
            Some(e) => e,
            None => self.expr(d, ctx, path)?,
        })
    }

    fn spell_ref(&self, name: &str, ctx: Ctx, path: &str) -> R<SpellId> {
        self.w.find_spell(ctx.class, name).ok_or_else(|| format!("{path}: unknown spell '{name}'"))
    }
    fn aura_ref(&self, name: &str, ctx: Ctx, path: &str) -> R<AuraId> {
        self.w.find_aura(ctx.class, name).ok_or_else(|| format!("{path}: unknown aura '{name}'"))
    }

    fn target_sel(&mut self, t: &Table, key: &str, default: &str, ctx: Ctx, path: &str) -> R<TargetSel> {
        let v = s(t, key).unwrap_or_else(|| default.to_string());
        Ok(match v.as_str() {
            "self" | "me" => TargetSel::Me,
            "target" => TargetSel::Target,
            "pet" => TargetSel::Pet,
            "owner" => TargetSel::Owner,
            "my_target" => TargetSel::MyTarget,
            "enemies" | "all_enemies" => {
                TargetSel::Enemies(self.exd(t, "max_targets", "100", ctx, path)?)
            }
            other => return Err(format!("{path}.{key}: unknown target '{other}'")),
        })
    }

    fn effects(&mut self, v: Option<&Value>, ctx: Ctx, path: &str) -> R<Vec<Effect>> {
        let mut out = Vec::new();
        for (i, e) in entries(v).into_iter().enumerate() {
            let p = format!("{path}[{i}]");
            out.push(self.effect(tbl(e, &p)?, ctx, &p)?);
        }
        Ok(out)
    }

    fn effect(&mut self, t: &Table, ctx: Ctx, path: &str) -> R<Effect> {
        let ty = s(t, "type").ok_or_else(|| format!("{path}: effect needs a `type`"))?;
        Ok(match ty.as_str() {
            "damage" => {
                let weapon = match s(t, "weapon").as_deref() {
                    None => None,
                    Some(h) => Some(Hand::from_name(h).ok_or_else(|| format!("{path}.weapon: mh, oh or ranged"))?),
                };
                let school = match s(t, "school") {
                    None => None,
                    Some(x) => Some(School::from_name(&x).ok_or_else(|| format!("{path}.school: unknown school '{x}'"))?),
                };
                let periodic = bl(t, "periodic", false);
                let hit = match s(t, "hit").as_deref() {
                    Some("white") => HitKind::White,
                    Some("special") | Some("melee") => HitKind::Special,
                    Some("ranged") => HitKind::RangedSpecial,
                    Some("ranged_white") => HitKind::RangedWhite,
                    Some("spell") => HitKind::Spell,
                    Some("auto") | Some("none") => HitKind::Auto,
                    Some(x) => return Err(format!("{path}.hit: unknown '{x}'")),
                    None => {
                        if periodic {
                            HitKind::Auto
                        } else if weapon == Some(Hand::Ranged) {
                            HitKind::RangedSpecial
                        } else if weapon.is_some() || school == Some(School::Physical) {
                            HitKind::Special
                        } else {
                            HitKind::Spell
                        }
                    }
                };
                let fx = DamageFx {
                    school,
                    hit,
                    weapon,
                    weapon_mult: self.ex(t, "weapon_mult", ctx, path)?,
                    normalized: bl(t, "normalized", false),
                    min: self.ex(t, "min", ctx, path)?,
                    max: self.ex(t, "max", ctx, path)?,
                    amount: self.ex(t, "amount", ctx, path)?,
                    coef: self.ex(t, "coef", ctx, path)?,
                    mult: self.ex(t, "mult", ctx, path)?,
                    crit: self.ex(t, "crit", ctx, path)?,
                    crit_bonus: self.ex(t, "crit_bonus", ctx, path)?,
                    can_crit: bl(t, "can_crit", !periodic),
                    threat: self.ex(t, "threat", ctx, path)?,
                    targets: self.target_sel(t, "target", "target", ctx, path)?,
                    tag: s(t, "tag"),
                    on_hit: self.effects(t.get("on_hit"), ctx, &format!("{path}.on_hit"))?,
                    on_crit: self.effects(t.get("on_crit"), ctx, &format!("{path}.on_crit"))?,
                    on_miss: self.effects(t.get("on_miss"), ctx, &format!("{path}.on_miss"))?,
                    lifesteal: self.ex(t, "lifesteal", ctx, path)?,
                    periodic,
                    no_proc: bl(t, "no_proc", false),
                };
                Effect::Damage(Box::new(fx))
            }
            "heal" => {
                let periodic = bl(t, "periodic", false);
                Effect::Heal(Box::new(HealFx {
                    targets: self.target_sel(t, "target", "self", ctx, path)?,
                    min: self.ex(t, "min", ctx, path)?,
                    max: self.ex(t, "max", ctx, path)?,
                    amount: self.ex(t, "amount", ctx, path)?,
                    coef: self.ex(t, "coef", ctx, path)?,
                    mult: self.ex(t, "mult", ctx, path)?,
                    can_crit: bl(t, "can_crit", !periodic && (t.contains_key("min") || t.contains_key("coef"))),
                    crit: self.ex(t, "crit", ctx, path)?,
                    tag: s(t, "tag"),
                    periodic,
                    on_crit: self.effects(t.get("on_crit"), ctx, &format!("{path}.on_crit"))?,
                }))
            }
            "resource" => {
                let r = s(t, "res").ok_or_else(|| format!("{path}: resource effect needs `res`"))?;
                Effect::Resource {
                    who: self.target_sel(t, "target", "self", ctx, path)?,
                    res: Res::from_name(&r).ok_or_else(|| format!("{path}.res: unknown resource '{r}'"))?,
                    amount: self.exd(t, "amount", "0", ctx, path)?,
                    set: bl(t, "set", false),
                }
            }
            "combo" => Effect::Combo(self.exd(t, "amount", "1", ctx, path)?),
            "reset_combo" => Effect::ResetCombo,
            "aura" | "apply_aura" => {
                let a = s(t, "aura").ok_or_else(|| format!("{path}: needs `aura`"))?;
                let id = self.aura_ref(&a, ctx, path)?;
                let def_who = if self.w.auras[id as usize].debuff { "target" } else { "self" };
                Effect::ApplyAura {
                    who: self.target_sel(t, "target", def_who, ctx, path)?,
                    aura: id,
                    stacks: self.exd(t, "stacks", "1", ctx, path)?,
                    duration: self.ex(t, "duration", ctx, path)?,
                }
            }
            "remove_aura" => {
                let a = s(t, "aura").ok_or_else(|| format!("{path}: needs `aura`"))?;
                let id = self.aura_ref(&a, ctx, path)?;
                Effect::RemoveAura { who: self.target_sel(t, "target", "self", ctx, path)?, aura: id }
            }
            "remove_self" => Effect::RemoveSelf,
            "consume" | "consume_charge" => Effect::ConsumeCharge,
            "emit" => {
                let ev = s(t, "event").ok_or_else(|| format!("{path}: emit needs `event`"))?;
                Effect::Emit(Self::intern(&mut self.w.customs, &ev))
            }
            "cooldown" => {
                let sp = s(t, "spell").ok_or_else(|| format!("{path}: needs `spell`"))?;
                let op = match s(t, "op").as_deref().unwrap_or("reset") {
                    "reset" => CdOp::Reset,
                    "reduce" => CdOp::Reduce,
                    "set" => CdOp::Set,
                    x => return Err(format!("{path}.op: unknown '{x}'")),
                };
                Effect::Cooldown { spell: self.spell_ref(&sp, ctx, path)?, op, value: self.exd(t, "value", "0", ctx, path)? }
            }
            "summon" => {
                let p = s(t, "pet").ok_or_else(|| format!("{path}: needs `pet`"))?;
                Effect::Summon(*self.pet_idx.get(&p).ok_or_else(|| format!("{path}: unknown pet '{p}'"))?)
            }
            "dismiss" => Effect::Dismiss,
            "sacrifice" => Effect::Sacrifice,
            "extra_attack" => Effect::ExtraAttack {
                hand: Hand::from_name(&s(t, "hand").unwrap_or("mh".into())).ok_or_else(|| format!("{path}.hand"))?,
                count: self.exd(t, "count", "1", ctx, path)?,
                ap_bonus: self.ex(t, "ap_bonus", ctx, path)?,
            },
            "cast" => {
                let sp = s(t, "spell").ok_or_else(|| format!("{path}: needs `spell`"))?;
                Effect::Cast { spell: self.spell_ref(&sp, ctx, path)?, who: self.target_sel(t, "target", "target", ctx, path)? }
            }
            "interrupt" => Effect::Interrupt {
                who: self.target_sel(t, "target", "target", ctx, path)?,
                lockout: self.exd(t, "lockout", "0", ctx, path)?,
            },
            "if" => Effect::If {
                cond: self.exd(t, "cond", "1", ctx, path)?,
                then: self.effects(t.get("then"), ctx, &format!("{path}.then"))?,
                els: self.effects(t.get("else"), ctx, &format!("{path}.else"))?,
            },
            "chance" => Effect::Chance {
                p: self.exd(t, "p", "1", ctx, path)?,
                then: self.effects(t.get("then"), ctx, &format!("{path}.then"))?,
            },
            "dispel" => {
                let mut mechs = Vec::new();
                for m in strs(t, "mechanics") {
                    mechs.push(Mechanic::from_name(&m).ok_or_else(|| format!("{path}: unknown mechanic '{m}'"))?);
                }
                Effect::Dispel { who: self.target_sel(t, "target", "self", ctx, path)?, mechanics: mechs }
            }
            "taunt" => Effect::Taunt { duration: self.exd(t, "duration", "3", ctx, path)? },
            "threat" => Effect::Threat { amount: self.exd(t, "amount", "0", ctx, path)? },
            "absorb" => Effect::Absorb { amount: self.exd(t, "amount", "0", ctx, path)? },
            other => return Err(format!("{path}: unknown effect type '{other}'")),
        })
    }

    fn stat_mods(&mut self, v: Option<&Value>, ctx: Ctx, path: &str) -> R<Vec<StatMod>> {
        let mut out = Vec::new();
        for (i, e) in entries(v).into_iter().enumerate() {
            let p = format!("{path}[{i}]");
            let t = tbl(e, &p)?;
            let names = strs(t, "stat");
            if names.is_empty() {
                return Err(format!("{p}: needs `stat`"));
            }
            let (op, key) = if t.contains_key("mul") { (ModOp::Mul, "mul") } else { (ModOp::Add, "add") };
            let value = self.ex(t, key, ctx, &p)?.ok_or_else(|| format!("{p}: needs `add` or `mul`"))?;
            for n in names {
                let stats: Vec<Stat> = match n.as_str() {
                    "crit" => vec![Stat::MeleeCrit, Stat::RangedCrit, Stat::SpellCrit],
                    "all_stats" => vec![Stat::Str, Stat::Agi, Stat::Sta, Stat::Int, Stat::Spi],
                    "all_stats_pct" => vec![Stat::StrPct, Stat::AgiPct, Stat::StaPct, Stat::IntPct, Stat::SpiPct],
                    "haste" => vec![Stat::MeleeHaste, Stat::RangedHaste, Stat::CastHaste],
                    "spell_power" => vec![Stat::Sp],
                    x => vec![Stat::from_name(x).ok_or_else(|| format!("{p}: unknown stat '{x}'"))?],
                };
                for st in stats {
                    out.push(StatMod { stat: st, op, value: value.clone() });
                }
            }
        }
        Ok(out)
    }

    fn spell_mods(&mut self, v: Option<&Value>, ctx: Ctx, path: &str) -> R<Vec<SpellMod>> {
        let mut out = Vec::new();
        for (i, e) in entries(v).into_iter().enumerate() {
            let p = format!("{path}[{i}]");
            let t = tbl(e, &p)?;
            let mut spells = Vec::new();
            for sp in strs(t, "spells") {
                spells.push(self.spell_ref(&sp, ctx, &p)?);
            }
            let school = match s(t, "school") {
                Some(x) => Some(School::from_name(&x).ok_or_else(|| format!("{p}.school"))?),
                None => None,
            };
            let prop = match s(t, "prop").as_deref() {
                Some("cost") => SpellProp::Cost,
                Some("cast_time") => SpellProp::CastTime,
                Some("cooldown") => SpellProp::Cooldown,
                Some("damage") => SpellProp::Damage,
                Some("heal") => SpellProp::Heal,
                Some("crit") => SpellProp::Crit,
                Some("crit_bonus") => SpellProp::CritBonus,
                Some("threat") => SpellProp::Threat,
                Some("gcd") => SpellProp::Gcd,
                x => return Err(format!("{p}.prop: unknown {x:?}")),
            };
            let (op, key) = if t.contains_key("mul") { (ModOp::Mul, "mul") } else { (ModOp::Add, "add") };
            let value = self.ex(t, key, ctx, &p)?.ok_or_else(|| format!("{p}: needs `add` or `mul`"))?;
            out.push(SpellMod { spells, school, prop, op, value });
        }
        Ok(out)
    }

    fn event_kind(&mut self, on: &str, ctx: Ctx, path: &str) -> R<EventKind> {
        if let Some(sp) = on.strip_prefix("cast:") {
            return Ok(EventKind::Cast(self.spell_ref(sp, ctx, path)?));
        }
        if let Some(c) = on.strip_prefix("custom:") {
            return Ok(EventKind::Custom(Self::intern(&mut self.w.customs, c)));
        }
        if let Some(a) = on.strip_prefix("aura_applied:") {
            return Ok(EventKind::AuraApplied(self.aura_ref(a, ctx, path)?));
        }
        if let Some(a) = on.strip_prefix("aura_expired:") {
            return Ok(EventKind::AuraExpired(self.aura_ref(a, ctx, path)?));
        }
        Ok(match on {
            "melee_swing" => EventKind::MeleeSwing,
            "melee_landed" => EventKind::MeleeLanded,
            "melee_crit" => EventKind::MeleeCrit,
            "white_landed" => EventKind::WhiteLanded,
            "special_landed" => EventKind::SpecialLanded,
            "ranged_landed" => EventKind::RangedLanded,
            "spell_landed" => EventKind::SpellLanded,
            "spell_crit" => EventKind::SpellCrit,
            "periodic" => EventKind::Periodic,
            "heal" => EventKind::Heal,
            "heal_crit" => EventKind::HealCrit,
            "damage_dealt" => EventKind::DamageDealt,
            "struck" => EventKind::Struck,
            "struck_melee" => EventKind::StruckMelee,
            "struck_crit" => EventKind::StruckCrit,
            "dodge" => EventKind::Dodge,
            "parry" => EventKind::Parry,
            "block" => EventKind::Block,
            "attack_dodged" => EventKind::AttackDodged,
            "attack_parried" => EventKind::AttackParried,
            "kill" => EventKind::Kill,
            "cast" => EventKind::AnyCast,
            "combat_start" => EventKind::CombatStart,
            "summon" => EventKind::Summon,
            "pet_died" => EventKind::PetDied,
            x => return Err(format!("{path}.on: unknown event '{x}'")),
        })
    }

    fn procs(&mut self, v: Option<&Value>, ctx: Ctx, path: &str) -> R<Vec<Proc>> {
        let mut out = Vec::new();
        for (i, e) in entries(v).into_iter().enumerate() {
            let p = format!("{path}[{i}]");
            let t = tbl(e, &p)?;
            let on = s(t, "on").ok_or_else(|| format!("{p}: needs `on`"))?;
            self.proc_key += 1;
            out.push(Proc {
                on: self.event_kind(&on, ctx, &p)?,
                cond: self.ex(t, "if", ctx, &p)?,
                chance: self.ex(t, "chance", ctx, &p)?,
                ppm: self.ex(t, "ppm", ctx, &p)?,
                icd: f(t, "icd", 0.0),
                effects: self.effects(t.get("effects"), ctx, &format!("{p}.effects"))?,
                key: self.proc_key,
            });
        }
        Ok(out)
    }

    fn passive(&mut self, t: &Table, ctx: Ctx, path: &str, name: &str, cond: Option<Expr>) -> R<Passive> {
        let c = match self.ex(t, "if", ctx, path)? {
            Some(e) => Some(e),
            None => cond,
        };
        Ok(Passive {
            name: s(t, "name").unwrap_or_else(|| name.to_string()),
            cond: c,
            mods: self.stat_mods(t.get("mods"), ctx, &format!("{path}.mods"))?,
            spell_mods: self.spell_mods(t.get("spell_mods"), ctx, &format!("{path}.spell_mods"))?,
            procs: self.procs(t.get("procs"), ctx, &format!("{path}.procs"))?,
        })
    }

    fn passives(&mut self, v: Option<&Value>, ctx: Ctx, path: &str) -> R<Vec<Passive>> {
        let mut out = Vec::new();
        match v {
            Some(Value::Table(t)) => {
                for (k, pv) in t {
                    let p = format!("{path}.{k}");
                    out.push(self.passive(tbl(pv, &p)?, ctx, &p, k, None)?);
                }
            }
            other => {
                for (i, e) in entries(other).into_iter().enumerate() {
                    let p = format!("{path}[{i}]");
                    out.push(self.passive(tbl(e, &p)?, ctx, &p, &format!("passive {i}"), None)?);
                }
            }
        }
        Ok(out)
    }

    fn rotation(&mut self, id: &str, t: &Table, ctx: Ctx, path: &str) -> R<Rotation> {
        let mut lists = Vec::new();
        for key in ["precombat", "opener", "actions", "rest"] {
            let mut acts = Vec::new();
            if let Some(Value::Array(a)) = t.get(key) {
                for (i, e) in a.iter().enumerate() {
                    let p = format!("{path}.{key}[{i}]");
                    let (sp, cond, target, line) = match e {
                        Value::String(x) => (x.clone(), None, None, x.clone()),
                        Value::Table(at) => {
                            let sp = s(at, "cast").ok_or_else(|| format!("{p}: needs `cast`"))?;
                            let line = match s(at, "if") {
                                Some(c) => format!("{sp} if {c}"),
                                None => sp.clone(),
                            };
                            (sp, self.ex(at, "if", ctx, &p)?, s(at, "target"), line)
                        }
                        _ => return Err(format!("{p}: expected a spell name or table")),
                    };
                    let spell = self.spell_ref(&sp, ctx, &p)?;
                    let tgt = match target.as_deref() {
                        None => self.w.spells[spell as usize].target,
                        Some("self") => SpellTarget::Me,
                        Some("target") | Some("enemy") => SpellTarget::Enemy,
                        Some("pet") => SpellTarget::Pet,
                        Some(x) => return Err(format!("{p}.target: unknown '{x}'")),
                    };
                    acts.push(Action { spell, cond, target: tgt, line });
                }
            }
            lists.push(acts);
        }
        let rest = lists.pop().unwrap();
        let actions = lists.pop().unwrap();
        let opener = lists.pop().unwrap();
        let precombat = lists.pop().unwrap();
        Ok(Rotation {
            id: id.to_string(),
            desc: s(t, "desc").unwrap_or_default(),
            auto_melee: bl(t, "auto_melee", true),
            auto_ranged: bl(t, "auto_ranged", false),
            precombat,
            opener,
            actions,
            rest,
            eat_below: f(t, "eat_below", 0.5),
            drink_below: f(t, "drink_below", 0.4),
        })
    }
}

fn rank_list(v: Option<&Value>, fields: &mut Vec<String>, path: &str) -> R<Vec<RankDef>> {
    let mut items: Vec<(String, &Table)> = Vec::new();
    match v {
        None => {}
        Some(Value::Array(a)) => {
            for (i, x) in a.iter().enumerate() {
                items.push((i.to_string(), tbl(x, path)?));
            }
        }
        Some(Value::Table(t)) => {
            for (k, x) in t {
                items.push((k.clone(), tbl(x, path)?));
            }
        }
        _ => return Err(format!("{path}: ranks must be a list or table")),
    }
    let mut out = Vec::new();
    for (k, t) in items {
        let level = t.get("level").and_then(|v| v.as_integer()).ok_or_else(|| format!("{path}.{k}: rank needs `level`"))?;
        let mut fl = Vec::new();
        for (fk, fv) in t {
            if fk == "level" {
                continue;
            }
            let x = match fv {
                Value::Integer(i) => *i as f64,
                Value::Float(x) => *x,
                _ => return Err(format!("{path}.{k}.{fk}: rank fields must be numbers")),
            };
            fl.push((Reg::intern(fields, fk), x));
        }
        out.push(RankDef { level: level as u32, fields: fl });
    }
    out.sort_by_key(|r| r.level);
    Ok(out)
}

fn rank_has(ranks: &[RankDef], fields: &[String], name: &str) -> bool {
    match fields.iter().position(|x| x == name) {
        Some(i) => ranks.iter().any(|r| r.fields.iter().any(|(f, _)| *f as usize == i)),
        None => false,
    }
}

fn parse_weapon(v: Option<&Value>, path: &str) -> R<Option<Weapon>> {
    let Some(v) = v else { return Ok(None) };
    let t = tbl(v, path)?;
    let wt = s(t, "type").unwrap_or("sword".into());
    Ok(Some(Weapon {
        min: f(t, "min", 1.0),
        max: f(t, "max", 2.0),
        speed: f(t, "speed", 2.0),
        wtype: WeaponType::from_name(&wt).ok_or_else(|| format!("{path}.type: unknown weapon type '{wt}'"))?,
    }))
}

pub fn parse_gear_stats(t: &Table, path: &str) -> R<GearStats> {
    let mut g = GearStats::default();
    for (k, v) in t {
        match k.as_str() {
            "level" | "desc" => {}
            "mh" => g.mh = parse_weapon(Some(v), &format!("{path}.mh"))?,
            "oh" => g.oh = parse_weapon(Some(v), &format!("{path}.oh"))?,
            "ranged" => g.ranged = parse_weapon(Some(v), &format!("{path}.ranged"))?,
            "shield" => {
                let st = tbl(v, path)?;
                g.shield = Some((f(st, "armor", 0.0), f(st, "block_value", 0.0)));
            }
            name => {
                let x = src_of(v).and_then(|s| s.parse::<f64>().ok()).ok_or_else(|| format!("{path}.{k}: expected a number"))?;
                let stats: Vec<Stat> = match name {
                    "crit" => vec![Stat::MeleeCrit, Stat::RangedCrit, Stat::SpellCrit],
                    other => vec![Stat::from_name(other).ok_or_else(|| format!("{path}: unknown gear stat '{other}'"))?],
                };
                for st in stats {
                    g.stats.push((st, x));
                }
            }
        }
    }
    Ok(g)
}

const SPELL_KEYS: &[&str] = &[
    "name", "school", "talent", "level", "race", "ranks", "cost", "cast_time", "channel", "gcd", "off_gcd",
    "cooldown", "cd_group", "requires", "forms", "next_swing", "target", "effects", "finisher", "usable_while",
    "source", "text",
];
const AURA_KEYS: &[&str] = &[
    "name", "duration", "max_stacks", "charges", "per_source", "debuff", "mechanic", "break_on_damage", "tick",
    "mods", "spell_mods", "procs", "on_apply", "on_refresh", "on_expire", "on_remove", "absorb", "group", "form",
    "consume_on_cast", "refresh", "dispel", "text", "source",
];

fn check_keys(t: &Table, allowed: &[&str], path: &str) -> R<()> {
    for k in t.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(format!("{path}: unknown field '{k}' (allowed: {})", allowed.join(", ")));
        }
    }
    Ok(())
}

pub fn compile(tree: Value, meta: WorldMeta) -> R<World> {
    let root = tbl(&tree, "data")?;
    let c = sub(root, "combat").ok_or("data: missing [combat]")?;
    let combat = CombatCfg {
        rage_conv: {
            let v = num_list(c.get("rage_conversion"));
            [v.first().copied().unwrap_or(0.0091107836), v.get(1).copied().unwrap_or(3.225598133), v.get(2).copied().unwrap_or(4.2652911)]
        },
        rage_dealt: f(c, "rage_dealt", 7.5),
        rage_taken: f(c, "rage_taken", 2.5),
        melee_miss: num_list(c.get("melee_miss")),
        mob_dodge: num_list(c.get("mob_dodge")),
        mob_parry: num_list(c.get("mob_parry")),
        glance_chance: num_list(c.get("glance_chance")),
        glance_mult: num_list(c.get("glance_mult")),
        crit_suppression: num_list(c.get("crit_suppression")),
        spell_hit: num_list(c.get("spell_hit")),
        spell_hit_cap: f(c, "spell_hit_cap", 99.0),
        dual_wield_miss: f(c, "dual_wield_miss", 19.0),
        defense_per_point: f(c, "defense_per_point", 0.04),
        gcd: f(c, "gcd", 1.5),
        energy_gcd: f(c, "energy_gcd", 1.0),
        pushback: f(c, "pushback", 0.5),
        max_pushbacks: f(c, "max_pushbacks", 2.0) as u32,
        heal_to_spell_damage: f(c, "heal_to_spell_damage", 1.0 / 3.0),
        melee_crit_mult: f(c, "melee_crit_mult", 2.0),
        spell_crit_mult: f(c, "spell_crit_mult", 1.5),
        downrank_offset: f(c, "downrank_offset", 11.0),
        armor_cap: f(c, "armor_cap", 0.75),
        armor_k_per_level: f(c, "armor_k_per_level", 85.0),
        low_level_penalty_level: f(c, "low_level_penalty_level", 20.0),
        dr_window: f(c, "dr_window", 15.0),
        low_level_penalty_per: f(c, "low_level_penalty_per", 0.0375),
        armor_k_base: f(c, "armor_k_base", 400.0),
        tick: f(c, "tick", 2.0),
        energy_per_tick: f(c, "energy_per_tick", 20.0),
        five_second_rule: f(c, "five_second_rule", 5.0),
        normalized_1h: f(c, "normalized_1h", 2.4),
        normalized_2h: f(c, "normalized_2h", 3.3),
        normalized_dagger: f(c, "normalized_dagger", 1.7),
        normalized_ranged: f(c, "normalized_ranged", 2.8),
        block_crit_allowed: bl(c, "block_crit_allowed", false),
    };

    let mut opts = Vec::new();
    if let Some(o) = sub(root, "options") {
        for (k, v) in o {
            let (d, desc) = match v {
                Value::Table(t) => (f(t, "default", 0.0), s(t, "desc").unwrap_or_default()),
                other => (src_of(other).and_then(|x| x.parse().ok()).unwrap_or(0.0), String::new()),
            };
            opts.push(OptDef { name: k.clone(), default: d, desc });
        }
    }
    let rest = {
        let rt = sub(root, "rest").ok_or("data: missing [rest]")?;
        let mut table = Vec::new();
        if let Some(Value::Array(a)) = rt.get("table") {
            for row in a {
                let v = num_list(Some(row));
                if v.len() == 4 {
                    table.push((v[0] as u32, v[1], v[2], v[3]));
                }
            }
        }
        if table.is_empty() {
            return Err("rest.table is empty".into());
        }
        RestCfg { table }
    };

    let world = World {
        meta,
        combat,
        opts,
        classes: vec![],
        races: vec![],
        spells: vec![],
        auras: vec![],
        pets: vec![],
        forms: vec![],
        groups: vec![],
        customs: vec![],
        rank_fields: vec![],
        gear: BTreeMap::new(),
        enemies: BTreeMap::new(),
        scenarios: BTreeMap::new(),
        rest,
        common_passives: vec![],
        spell_index: HashMap::new(),
        aura_index: HashMap::new(),
    };
    let opt_idx = world.opts.iter().enumerate().map(|(i, o)| (o.name.clone(), i as u16)).collect();
    let mut reg = Reg { w: world, opt_idx, form_idx: HashMap::new(), pet_idx: HashMap::new(), proc_key: 0 };

    let empty = Table::new();
    let classes_t = sub(root, "classes").unwrap_or(&empty);
    let races_t = sub(root, "races").unwrap_or(&empty);
    let common_t = sub(root, "common").unwrap_or(&empty);

    // ------------------------------------------------ pass 1: register names
    // (class idx, spell table, qualified id, plain id)
    let mut spell_src: Vec<(Option<usize>, Option<u16>, &Table, String)> = Vec::new();
    let mut aura_src: Vec<(Option<usize>, &Table, String)> = Vec::new();

    for (ri, (rid, rv)) in races_t.iter().enumerate() {
        let rt = tbl(rv, &format!("races.{rid}"))?;
        let ct = s(rt, "creature").unwrap_or("humanoid".into());
        let mut stats = [0.0; 5];
        if let Some(st) = sub(rt, "stats") {
            for (i, k) in ["str", "agi", "sta", "int", "spi"].iter().enumerate() {
                stats[i] = f(st, k, 0.0);
            }
        }
        reg.w.races.push(RaceDef {
            id: rid.clone(),
            name: s(rt, "name").unwrap_or(rid.clone()),
            icon: s(rt, "icon").unwrap_or_default(),
            faction: s(rt, "faction").unwrap_or_default(),
            classes: strs(rt, "classes"),
            stats,
            passives: vec![],
            creature: CreatureType::from_name(&ct).ok_or_else(|| format!("races.{rid}.creature"))?,
            source: s(rt, "source").unwrap_or_default(),
        });
        for (sid, sv) in sub(rt, "spells").unwrap_or(&empty) {
            spell_src.push((None, Some(ri as u16), tbl(sv, &format!("races.{rid}.spells.{sid}"))?, sid.clone()));
        }
        for (aid, av) in sub(rt, "auras").unwrap_or(&empty) {
            aura_src.push((None, tbl(av, &format!("races.{rid}.auras.{aid}"))?, aid.clone()));
        }
    }
    for (sid, sv) in sub(common_t, "spells").unwrap_or(&empty) {
        spell_src.push((None, None, tbl(sv, &format!("common.spells.{sid}"))?, sid.clone()));
    }
    for (aid, av) in sub(common_t, "auras").unwrap_or(&empty) {
        aura_src.push((None, tbl(av, &format!("common.auras.{aid}"))?, aid.clone()));
    }

    for (ci, (cid, cv)) in classes_t.iter().enumerate() {
        let path = format!("classes.{cid}");
        let ct = tbl(cv, &path)?;
        let mut talents = Vec::new();
        let mut talent_index = HashMap::new();
        let mut trees: Vec<String> = Vec::new();
        for (tid, tv) in sub(ct, "talents").unwrap_or(&empty) {
            let tt = tbl(tv, &format!("{path}.talents.{tid}"))?;
            let tree = s(tt, "tree").unwrap_or_default();
            if !trees.contains(&tree) {
                trees.push(tree.clone());
            }
            talent_index.insert(tid.clone(), talents.len() as u16);
            talents.push(TalentDef {
                id: tid.clone(),
                name: s(tt, "name").unwrap_or_else(|| pretty_name(tid)),
                tree,
                points: f(tt, "points", 0.0) as u32,
                ranks: f(tt, "ranks", 1.0) as u32,
                requires: None,
                text: s(tt, "text").unwrap_or_default(),
                icon: s(tt, "icon").unwrap_or_default(),
                source: s(tt, "source").unwrap_or_default(),
            });
        }
        // prerequisites
        for (tid, tv) in sub(ct, "talents").unwrap_or(&empty) {
            if let Some(req) = s(tbl(tv, "")?, "requires") {
                let ri = *talent_index.get(&req).ok_or_else(|| format!("{path}.talents.{tid}.requires: unknown talent '{req}'"))?;
                let me = talent_index[tid] as usize;
                talents[me].requires = Some(ri);
            }
        }
        let resources = strs(ct, "resources")
            .iter()
            .map(|r| Res::from_name(r).ok_or_else(|| format!("{path}.resources: unknown '{r}'")))
            .collect::<R<Vec<_>>>()?;
        let mut base = Vec::new();
        if let Some(Value::Array(a)) = ct.get("base") {
            for b in a {
                let bt = tbl(b, &format!("{path}.base"))?;
                base.push(BaseAnchor {
                    level: f(bt, "level", 1.0),
                    vals: [
                        f(bt, "str", 20.0), f(bt, "agi", 20.0), f(bt, "sta", 20.0), f(bt, "int", 20.0),
                        f(bt, "spi", 20.0), f(bt, "hp", 50.0), f(bt, "mana", 0.0),
                    ],
                });
            }
        }
        if base.is_empty() {
            return Err(format!("{path}: needs at least one `base` entry"));
        }
        base.sort_by(|a, b| a.level.partial_cmp(&b.level).unwrap());
        let dummy = Expr::constant(0.0);
        reg.w.classes.push(ClassDef {
            id: cid.clone(),
            name: s(ct, "name").unwrap_or(cid.clone()),
            icon: s(ct, "icon").unwrap_or_default(),
            resources,
            base,
            f: ClassFormulas {
                ap: dummy.clone(), rap: dummy.clone(), melee_crit: dummy.clone(), ranged_crit: dummy.clone(),
                spell_crit: dummy.clone(), dodge: dummy.clone(), parry: dummy.clone(), block: dummy.clone(),
                mana_regen: dummy.clone(), hp_regen: dummy,
            },
            talents,
            talent_index,
            trees,
            passives: vec![],
            rotations: BTreeMap::new(),
            can_parry: bl(ct, "can_parry", false),
            can_block: bl(ct, "can_block", false),
            can_dual_wield: bl(ct, "can_dual_wield", false),
            energy_gcd: bl(ct, "energy_gcd", false),
            source: s(ct, "source").unwrap_or_default(),
        });
        for (sid, sv) in sub(ct, "spells").unwrap_or(&empty) {
            spell_src.push((Some(ci), None, tbl(sv, &format!("{path}.spells.{sid}"))?, sid.clone()));
        }
        for (aid, av) in sub(ct, "auras").unwrap_or(&empty) {
            aura_src.push((Some(ci), tbl(av, &format!("{path}.auras.{aid}"))?, aid.clone()));
        }
        for (pid, _) in sub(ct, "pets").unwrap_or(&empty) {
            let n = reg.pet_idx.len() as u16;
            reg.pet_idx.insert(pid.clone(), n);
        }
    }

    let qual = |class: Option<usize>, id: &str, w: &World| match class {
        Some(c) => format!("{}.{}", w.classes[c].id, id),
        None => id.to_string(),
    };
    // register auras (forms & groups need indices before spells compile)
    for (class, at, id) in &aura_src {
        let q = qual(*class, id, &reg.w);
        if reg.w.aura_index.contains_key(&q) {
            return Err(format!("duplicate aura '{q}'"));
        }
        let form = if bl(at, "form", false) {
            let fi = Reg::intern(&mut reg.w.forms, id);
            reg.form_idx.insert(id.clone(), fi);
            Some(fi)
        } else {
            None
        };
        let group = s(at, "group").map(|g| Reg::intern(&mut reg.w.groups, &g));
        let dummy = Expr::constant(0.0);
        let debuff = bl(at, "debuff", false);
        reg.w.aura_index.insert(q.clone(), reg.w.auras.len() as u32);
        reg.w.auras.push(AuraDef {
            id: id.clone(),
            qid: q,
            name: s(at, "name").unwrap_or(id.clone()),
            class: *class,
            duration: dummy,
            max_stacks: f(at, "max_stacks", 1.0) as u32,
            charges: None,
            per_source: bl(at, "per_source", debuff),
            debuff,
            mechanic: None,
            break_on_damage: None,
            tick: None,
            mods: vec![],
            spell_mods: vec![],
            procs: vec![],
            on_apply: vec![],
            on_refresh: vec![],
            on_expire: vec![],
            on_remove: vec![],
            absorb: None,
            group,
            form,
            consume_on_cast: bl(at, "consume_on_cast", false),
            refresh: match s(at, "refresh").as_deref() {
                Some("ignore") => Refresh::Ignore,
                Some("replace") => Refresh::Replace,
                _ => Refresh::Refresh,
            },
            dispel_type: s(at, "dispel"),
            text: s(at, "text").unwrap_or_default(),
        });
    }
    for (class, race, st, id) in &spell_src {
        let q = qual(*class, id, &reg.w);
        if reg.w.spell_index.contains_key(&q) {
            return Err(format!("duplicate spell '{q}'"));
        }
        let path = match class {
            Some(c) => format!("classes.{}.spells.{id}", reg.w.classes[*c].id),
            None => format!("spells.{id}"),
        };
        check_keys(st, SPELL_KEYS, &path)?;
        let school = s(st, "school").unwrap_or("physical".into());
        let ranks = rank_list(st.get("ranks"), &mut reg.w.rank_fields, &format!("{path}.ranks"))?;
        let talent = match (s(st, "talent"), class) {
            (Some(tn), Some(c)) => Some(
                *reg.w.classes[*c].talent_index.get(&tn).ok_or_else(|| format!("{path}.talent: unknown talent '{tn}'"))?,
            ),
            (Some(_), None) => return Err(format!("{path}.talent: only class spells can require talents")),
            _ => None,
        };
        let forms = {
            let fs = strs(st, "forms");
            if fs.is_empty() {
                FormReq::Any
            } else if fs.iter().any(|x| x == "none") && fs.len() == 1 {
                FormReq::NoForm
            } else {
                let mut v = Vec::new();
                for fname in fs.iter().filter(|x| *x != "none") {
                    v.push(*reg.form_idx.get(fname).ok_or_else(|| format!("{path}.forms: unknown form '{fname}'"))?);
                }
                if fs.iter().any(|x| x == "none") {
                    v.push(u16::MAX);
                }
                FormReq::Forms(v)
            }
        };
        let dummy = Expr::constant(0.0);
        reg.w.spell_index.insert(q.clone(), reg.w.spells.len() as u32);
        reg.w.spells.push(SpellDef {
            id: id.clone(),
            qid: q,
            name: s(st, "name").unwrap_or(id.clone()),
            class: *class,
            race: *race,
            school: School::from_name(&school).ok_or_else(|| format!("{path}.school: unknown '{school}'"))?,
            talent,
            learn_level: f(st, "level", 1.0) as u32,
            ranks,
            cost: vec![],
            cast_time: dummy.clone(),
            channel: None,
            gcd: None,
            off_gcd: bl(st, "off_gcd", false),
            cooldown: dummy,
            cd_group: s(st, "cd_group").map(|g| Reg::intern(&mut reg.w.groups, &format!("cd:{g}")) as u32),
            requires: None,
            forms,
            next_swing: bl(st, "next_swing", false),
            target: SpellTarget::Enemy,
            effects: vec![],
            finisher: bl(st, "finisher", false),
            usable_while: strs(st, "usable_while").iter().filter_map(|m| Mechanic::from_name(m)).collect(),
            is_attack: false,
            source: s(st, "source").unwrap_or_default(),
            text: s(st, "text").unwrap_or_default(),
        });
    }

    // ------------------------------------------------ pass 2: compile bodies
    for (ai, (class, at, id)) in aura_src.iter().enumerate() {
        let ctx = Ctx { class: *class, rank_alias: None };
        let path = match class {
            Some(c) => format!("classes.{}.auras.{id}", reg.w.classes[*c].id),
            None => format!("auras.{id}"),
        };
        check_keys(at, AURA_KEYS, &path)?;
        let duration = reg.exd(at, "duration", "0", ctx, &path)?;
        let charges = reg.ex(at, "charges", ctx, &path)?;
        let mechanic = match s(at, "mechanic") {
            Some(m) => Some(Mechanic::from_name(&m).ok_or_else(|| format!("{path}.mechanic: unknown '{m}'"))?),
            None => None,
        };
        let break_on_damage = match at.get("break_on_damage") {
            Some(Value::Boolean(true)) => Some(0.0),
            Some(Value::Boolean(false)) | None => None,
            Some(v) => Some(src_of(v).and_then(|x| x.parse().ok()).unwrap_or(0.0)),
        };
        let tick = match sub(at, "tick") {
            Some(tt) => Some((
                reg.exd(tt, "interval", "3", ctx, &format!("{path}.tick"))?,
                reg.effects(tt.get("effects"), ctx, &format!("{path}.tick.effects"))?,
            )),
            None => None,
        };
        let mods = reg.stat_mods(at.get("mods"), ctx, &format!("{path}.mods"))?;
        let spell_mods = reg.spell_mods(at.get("spell_mods"), ctx, &format!("{path}.spell_mods"))?;
        let procs = reg.procs(at.get("procs"), ctx, &format!("{path}.procs"))?;
        let on_apply = reg.effects(at.get("on_apply"), ctx, &format!("{path}.on_apply"))?;
        let on_refresh = reg.effects(at.get("on_refresh"), ctx, &format!("{path}.on_refresh"))?;
        let on_expire = reg.effects(at.get("on_expire"), ctx, &format!("{path}.on_expire"))?;
        let on_remove = reg.effects(at.get("on_remove"), ctx, &format!("{path}.on_remove"))?;
        let absorb = reg.ex(at, "absorb", ctx, &path)?;
        let a = &mut reg.w.auras[ai];
        a.duration = duration;
        a.charges = charges;
        a.mechanic = mechanic;
        a.break_on_damage = break_on_damage;
        a.tick = tick;
        a.mods = mods;
        a.spell_mods = spell_mods;
        a.procs = procs;
        a.on_apply = on_apply;
        a.on_refresh = on_refresh;
        a.on_expire = on_expire;
        a.on_remove = on_remove;
        a.absorb = absorb;
    }
    for (si, (class, _race, st, id)) in spell_src.iter().enumerate() {
        let ctx = Ctx { class: *class, rank_alias: None };
        let path = match class {
            Some(c) => format!("classes.{}.spells.{id}", reg.w.classes[*c].id),
            None => format!("spells.{id}"),
        };
        let ranks = reg.w.spells[si].ranks.clone();
        let mut cost = Vec::new();
        if let Some(ct) = sub(st, "cost") {
            for (k, v) in ct {
                let r = Res::from_name(k).ok_or_else(|| format!("{path}.cost: unknown resource '{k}'"))?;
                let src = src_of(v).ok_or_else(|| format!("{path}.cost.{k}"))?;
                cost.push((r, reg.expr(&src, ctx, &format!("{path}.cost.{k}"))?));
            }
        } else {
            for r in [Res::Mana, Res::Rage, Res::Energy] {
                if rank_has(&ranks, &reg.w.rank_fields, r.name()) {
                    cost.push((r, reg.expr(&format!("r.{}", r.name()), ctx, &path)?));
                }
            }
        }
        let cast_default = if rank_has(&ranks, &reg.w.rank_fields, "cast") { "r.cast" } else { "0" };
        let cast_time = reg.exd(st, "cast_time", cast_default, ctx, &path)?;
        let cd_default = if rank_has(&ranks, &reg.w.rank_fields, "cd") { "r.cd" } else { "0" };
        let cooldown = reg.exd(st, "cooldown", cd_default, ctx, &path)?;
        let gcd = reg.ex(st, "gcd", ctx, &path)?;
        let requires = reg.ex(st, "requires", ctx, &path)?;
        let effects = reg.effects(st.get("effects"), ctx, &format!("{path}.effects"))?;
        let channel = match sub(st, "channel") {
            Some(ch) => Some(ChannelDef {
                duration: reg.exd(ch, "duration", "5", ctx, &format!("{path}.channel"))?,
                interval: reg.exd(ch, "interval", "1", ctx, &format!("{path}.channel"))?,
                effects: reg.effects(ch.get("effects"), ctx, &format!("{path}.channel.effects"))?,
            }),
            None => None,
        };
        let has_dmg = effects.iter().any(|e| matches!(e, Effect::Damage(_)))
            || channel.as_ref().map_or(false, |c| c.effects.iter().any(|e| matches!(e, Effect::Damage(_))))
            || effects.iter().any(|e| matches!(e, Effect::ApplyAura { who: TargetSel::Target, .. }));
        let target = match s(st, "target").as_deref() {
            None => {
                if has_dmg {
                    SpellTarget::Enemy
                } else {
                    SpellTarget::Me
                }
            }
            Some("enemy") | Some("target") => SpellTarget::Enemy,
            Some("self") => SpellTarget::Me,
            Some("friendly") => SpellTarget::Friendly,
            Some("pet") => SpellTarget::Pet,
            Some(x) => return Err(format!("{path}.target: unknown '{x}'")),
        };
        let is_attack = effects.iter().any(|e| matches!(e, Effect::Damage(d) if d.weapon.is_some()));
        let sp = &mut reg.w.spells[si];
        sp.cost = cost;
        sp.cast_time = cast_time;
        sp.cooldown = cooldown;
        sp.gcd = gcd;
        sp.requires = requires;
        sp.effects = effects;
        sp.channel = channel;
        sp.target = target;
        sp.is_attack = is_attack;
    }

    // classes: formulas, talents' own effects, passives, rotations, pets
    for (ci, (cid, cv)) in classes_t.iter().enumerate() {
        let path = format!("classes.{cid}");
        let ct = tbl(cv, &path)?;
        let ctx = Ctx { class: Some(ci), rank_alias: None };
        let ft = sub(ct, "formulas").unwrap_or(&empty);
        let fp = format!("{path}.formulas");
        let formulas = ClassFormulas {
            ap: reg.exd(ft, "ap", "str * 2 + level * 3 - 20", ctx, &fp)?,
            rap: reg.exd(ft, "rap", "agi + level * 2 - 10", ctx, &fp)?,
            melee_crit: reg.exd(ft, "melee_crit", "agi / 20", ctx, &fp)?,
            ranged_crit: reg.exd(ft, "ranged_crit", "agi / 20", ctx, &fp)?,
            spell_crit: reg.exd(ft, "spell_crit", "int / 60", ctx, &fp)?,
            dodge: reg.exd(ft, "dodge", "agi / 20", ctx, &fp)?,
            parry: reg.exd(ft, "parry", "0", ctx, &fp)?,
            block: reg.exd(ft, "block", "0", ctx, &fp)?,
            mana_regen: reg.exd(ft, "mana_regen", "15 + spi / 5", ctx, &fp)?,
            hp_regen: reg.exd(ft, "hp_regen", "spi * 0.5", ctx, &fp)?,
        };
        let mut passives = reg.passives(ct.get("passives"), ctx, &format!("{path}.passives"))?;
        for (tid, tv) in sub(ct, "talents").unwrap_or(&empty) {
            let tt = tbl(tv, "")?;
            if tt.contains_key("mods") || tt.contains_key("spell_mods") || tt.contains_key("procs") {
                let ti = reg.w.classes[ci].talent_index[tid];
                let tctx = Ctx { class: Some(ci), rank_alias: Some(ti) };
                let cond = reg.expr(&format!("t.{tid} > 0"), tctx, &path)?;
                let p = reg.passive(tt, tctx, &format!("{path}.talents.{tid}"), tid, Some(cond))?;
                passives.push(p);
            }
        }
        let mut rotations = BTreeMap::new();
        for (rid, rv) in sub(ct, "rotations").unwrap_or(&empty) {
            let p = format!("{path}.rotations.{rid}");
            rotations.insert(rid.clone(), reg.rotation(rid, tbl(rv, &p)?, ctx, &p)?);
        }
        for (pid, pv) in sub(ct, "pets").unwrap_or(&empty) {
            let p = format!("{path}.pets.{pid}");
            let pt = tbl(pv, &p)?;
            let rotation = match sub(pt, "rotation") {
                Some(rt) => Some(reg.rotation(pid, rt, ctx, &format!("{p}.rotation"))?),
                None => None,
            };
            let ctn = s(pt, "creature").unwrap_or("beast".into());
            let pet = PetDef {
                id: pid.clone(),
                name: s(pt, "name").unwrap_or(pid.clone()),
                class: ci,
                hp: reg.exd(pt, "hp", "100", ctx, &p)?,
                armor: reg.exd(pt, "armor", "0", ctx, &p)?,
                dmg_min: reg.exd(pt, "dmg_min", "1", ctx, &p)?,
                dmg_max: reg.exd(pt, "dmg_max", "2", ctx, &p)?,
                speed: f(pt, "speed", 2.0),
                mana: reg.exd(pt, "mana", "0", ctx, &p)?,
                ap: reg.exd(pt, "ap", "0", ctx, &p)?,
                sp: reg.exd(pt, "sp", "0", ctx, &p)?,
                crit: reg.exd(pt, "crit", "5", ctx, &p)?,
                dodge: reg.exd(pt, "dodge", "5", ctx, &p)?,
                creature: CreatureType::from_name(&ctn).ok_or_else(|| format!("{p}.creature"))?,
                mods: reg.stat_mods(pt.get("mods"), ctx, &format!("{p}.mods"))?,
                procs: reg.procs(pt.get("procs"), ctx, &format!("{p}.procs"))?,
                rotation,
                auto_melee: bl(pt, "auto_melee", true),
            };
            reg.w.pets.push(pet);
        }
        let cl = &mut reg.w.classes[ci];
        cl.f = formulas;
        cl.passives = passives;
        cl.rotations = rotations;
    }
    for (ri, (rid, rv)) in races_t.iter().enumerate() {
        let rt = tbl(rv, "")?;
        let ctx = Ctx { class: None, rank_alias: None };
        let p = reg.passives(rt.get("passives"), ctx, &format!("races.{rid}.passives"))?;
        reg.w.races[ri].passives = p;
    }
    let cp = reg.passives(common_t.get("passives"), Ctx { class: None, rank_alias: None }, "common.passives")?;
    reg.w.common_passives = cp;

    for (gid, gv) in sub(root, "gear").unwrap_or(&empty) {
        let p = format!("gear.{gid}");
        let gt = tbl(gv, &p)?;
        let mut anchors = Vec::new();
        match gt.get("levels") {
            Some(Value::Array(a)) => {
                for (i, x) in a.iter().enumerate() {
                    let xt = tbl(x, &format!("{p}.levels[{i}]"))?;
                    anchors.push((f(xt, "level", 60.0) as u32, parse_gear_stats(xt, &format!("{p}.levels[{i}]"))?));
                }
            }
            Some(Value::Table(t)) => {
                for (k, x) in t {
                    let lv: u32 = k.parse().map_err(|_| format!("{p}.levels: keys must be levels"))?;
                    anchors.push((lv, parse_gear_stats(tbl(x, &p)?, &format!("{p}.levels.{k}"))?));
                }
            }
            _ => return Err(format!("{p}: needs `levels`")),
        }
        anchors.sort_by_key(|a| a.0);
        reg.w.gear.insert(gid.clone(), GearKit { id: gid.clone(), desc: s(gt, "desc").unwrap_or_default(), anchors });
    }

    for (eid, ev) in sub(root, "enemies").unwrap_or(&empty) {
        let p = format!("enemies.{eid}");
        let et = tbl(ev, &p)?;
        let ctx = Ctx { class: None, rank_alias: None };
        let rotation = match sub(et, "rotation") {
            Some(rt) => Some(reg.rotation(eid, rt, ctx, &format!("{p}.rotation"))?),
            None => None,
        };
        let mut stuns = Vec::new();
        if let Some(Value::Array(a)) = et.get("stuns") {
            for x in a {
                let v = num_list(Some(x));
                if v.len() == 2 {
                    stuns.push((v[0], v[1]));
                }
            }
        }
        let ctn = s(et, "creature").unwrap_or("humanoid".into());
        let school = s(et, "school").unwrap_or("physical".into());
        let e = EnemyDef {
            id: eid.clone(),
            name: s(et, "name").unwrap_or(eid.clone()),
            creature: CreatureType::from_name(&ctn).ok_or_else(|| format!("{p}.creature"))?,
            level_offset: f(et, "level_offset", 0.0) as i32,
            hp: reg.exd(et, "hp", "100", ctx, &p)?,
            armor: reg.exd(et, "armor", "0", ctx, &p)?,
            dmg: reg.exd(et, "dmg", "10", ctx, &p)?,
            dmg_spread: f(et, "dmg_spread", 0.15),
            speed: f(et, "speed", 2.0),
            crit: f(et, "crit", 5.0),
            dodge: reg.ex(et, "dodge", ctx, &p)?,
            parry: reg.ex(et, "parry", ctx, &p)?,
            block: f(et, "block", 0.0),
            attacks: bl(et, "attacks", true),
            boss: bl(et, "boss", false),
            player_like: bl(et, "player_like", false),
            behind: bl(et, "behind", false),
            school: School::from_name(&school).ok_or_else(|| format!("{p}.school"))?,
            rotation,
            stuns,
            mods: reg.stat_mods(et.get("mods"), ctx, &format!("{p}.mods"))?,
        };
        reg.w.enemies.insert(eid.clone(), e);
    }

    for (sid, sv) in sub(root, "scenarios").unwrap_or(&empty) {
        let p = format!("scenarios.{sid}");
        let st = tbl(sv, &p)?;
        let kind = match s(st, "kind").as_deref() {
            Some("leveling") => ScenarioKind::Leveling {
                pull: f(st, "pull", 1.0) as u32,
                minutes: f(st, "minutes", 60.0),
                enemy: s(st, "enemy").unwrap_or("humanoid".into()),
                travel: f(st, "travel", 8.0),
            },
            Some("boss") => ScenarioKind::Boss {
                duration: f(st, "duration", 120.0),
                enemy: s(st, "enemy").unwrap_or("boss".into()),
                execute_by_time: bl(st, "execute_by_time", true),
            },
            Some("duel") => ScenarioKind::Duel { max_time: f(st, "max_time", 180.0), opponent: s(st, "opponent") },
            Some("pressure") => ScenarioKind::Pressure {
                attacker: s(st, "attacker").unwrap_or("pressure_rogue".into()),
                attackers: f(st, "attackers", 1.0) as u32,
                lo: f(st, "lo", 0.05),
                hi: f(st, "hi", 6.0),
                steps: f(st, "steps", 12.0) as u32,
                max_time: f(st, "max_time", 180.0),
                win_needs_kill: bl(st, "win_needs_kill", true),
            },
            x => return Err(format!("{p}.kind: unknown {x:?} (leveling, boss, duel, pressure)")),
        };
        let mut sopts = Vec::new();
        if let Some(ot) = sub(st, "opts") {
            for (k, v) in ot {
                let i = reg.opt_idx.get(k).ok_or_else(|| format!("{p}.opts: unknown option '{k}'"))?;
                sopts.push((*i, src_of(v).and_then(|x| x.parse().ok()).unwrap_or(0.0)));
            }
        }
        let primary_default = match &kind {
            ScenarioKind::Leveling { .. } => "kills_per_hour",
            ScenarioKind::Boss { .. } => "dps",
            ScenarioKind::Duel { .. } => "win",
            ScenarioKind::Pressure { .. } => "breakeven",
        };
        reg.w.scenarios.insert(
            sid.clone(),
            ScenarioDef {
                id: sid.clone(),
                desc: s(st, "desc").unwrap_or_default(),
                kind,
                primary: s(st, "primary").unwrap_or(primary_default.into()),
                behind: bl(st, "behind", false),
                opts: sopts,
            },
        );
    }

    // sanity: every enemy referenced by a scenario exists
    for sc in reg.w.scenarios.values() {
        let e = match &sc.kind {
            ScenarioKind::Leveling { enemy, .. } | ScenarioKind::Boss { enemy, .. } => Some(enemy),
            ScenarioKind::Pressure { attacker, .. } => Some(attacker),
            _ => None,
        };
        if let Some(e) = e {
            if !reg.w.enemies.contains_key(e) {
                return Err(format!("scenarios.{}: unknown enemy '{e}'", sc.id));
            }
        }
    }
    Ok(reg.w)
}
