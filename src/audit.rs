//! `fsim audit`: checks the class files against WoW: Forever's own spell
//! tables (data/reference/spellbook.json, written by tools/forever_db.mjs).
//!
//! Every spell rank is matched to the game's spell (by an `id` field on the
//! rank, or by class, name and level), and its numbers are worked out with the
//! engine's own formulas: the rank's fields, default assumptions, no talents,
//! the character at the rank's level. Anything that depends on stats or the
//! fight (attack power, combo points, ...) cannot be checked and is skipped.
//! Differences that are deliberate are listed, with a reason, in
//! data/reference/audit-accepted.toml.

use crate::expr::{Env, Expr};
use crate::model::*;
use crate::vars::{Res, VarRef};
use serde_json::Value as J;
use std::collections::HashMap;

/// The game's spells, by id and by (class, lower-case name).
pub struct Book {
    pub build: String,
    by_id: HashMap<u64, J>,
    by_name: HashMap<(String, String), Vec<u64>>,
    /// Forever's talent trees: class -> talents.
    talents: HashMap<String, Vec<GameTalent>>,
}

/// A talent as the game's trait tables describe it.
#[derive(Clone, Debug)]
pub struct GameTalent {
    pub name: String,
    pub spell: u64,
    pub ranks: u32,
    pub points: u32,
    pub requires: Vec<String>,
    /// For each effect of the talent's spell: its value at rank 1, 2, ...
    pub values: Vec<(u32, Vec<f64>)>,
}

/// Names compared without case, spaces or punctuation.
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

impl Book {
    pub fn parse(text: &str) -> Result<Book, String> {
        let v: J = serde_json::from_str(text).map_err(|e| format!("spellbook: {e}"))?;
        let spells = v.get("spells").and_then(|s| s.as_object()).ok_or("spellbook: no spells")?;
        let mut by_id = HashMap::new();
        let mut by_name: HashMap<(String, String), Vec<u64>> = HashMap::new();
        for s in spells.values() {
            let id = s.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
            let name = s.get("name").and_then(|x| x.as_str()).unwrap_or("").to_lowercase();
            let class = s.get("class").and_then(|x| x.as_str()).unwrap_or("").to_string();
            by_name.entry((class, name)).or_default().push(id);
            by_id.insert(id, s.clone());
        }
        for ids in by_name.values_mut() {
            ids.sort();
        }
        let mut talents: HashMap<String, Vec<GameTalent>> = HashMap::new();
        for (class, list) in v.get("talents").and_then(|t| t.as_object()).into_iter().flatten() {
            for t in list.as_array().into_iter().flatten() {
                let values = t
                    .get("values")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .map(|e| {
                                let ranks = e.get("ranks").and_then(|r| r.as_array()).map(|r| r.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
                                (e.get("effect").and_then(|x| x.as_u64()).unwrap_or(0) as u32, ranks)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                talents.entry(class.clone()).or_default().push(GameTalent {
                    name: t.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    spell: t.get("spell").and_then(|x| x.as_u64()).unwrap_or(0),
                    ranks: t.get("ranks").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                    points: t.get("points").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                    requires: t.get("requires").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
                    values,
                });
            }
        }
        Ok(Book { build: v.get("build").and_then(|x| x.as_str()).unwrap_or("").to_string(), by_id, by_name, talents })
    }
    pub fn get(&self, id: u64) -> Option<&J> {
        self.by_id.get(&id)
    }
    /// The game's player spell for a rank: same class and name, learned at the
    /// same level. Copies that cost nothing and take no time (the game's
    /// free "overload" and proc versions) lose to the castable one.
    fn find(&self, class: &str, name: &str, level: u32) -> Option<&J> {
        let ids = self.by_name.get(&(class.to_string(), name.to_lowercase()))?;
        let at: Vec<&J> = ids.iter().filter_map(|i| self.by_id.get(i)).filter(|s| lvl(s) == level).collect();
        at.iter().find(|s| s.get("cost").is_some() || s.get("cast").is_some()).or_else(|| at.first()).copied()
    }
    /// The game's castable ranks of a class spell, lowest level first (one
    /// per level; learnable spells only).
    fn ranks(&self, class: &str, name: &str) -> Vec<&J> {
        let Some(ids) = self.by_name.get(&(class.to_string(), name.to_lowercase())) else { return vec![] };
        let mut v: Vec<&J> = ids
            .iter()
            .filter_map(|i| self.by_id.get(i))
            .filter(|s| s.get("skill").is_some() && (s.get("cost").is_some() || s.get("cast").is_some() || s.get("gcd").is_some()))
            .collect();
        v.sort_by_key(|s| (lvl(s), num(s, "id").unwrap_or(0.0) as u64));
        v.dedup_by_key(|s| lvl(s));
        v
    }
    fn levels(&self, class: &str, name: &str) -> Vec<u32> {
        let mut v: Vec<u32> = self
            .by_name
            .get(&(class.to_string(), name.to_lowercase()))
            .map(|ids| ids.iter().filter_map(|i| self.by_id.get(i)).map(lvl).collect())
            .unwrap_or_default();
        v.sort();
        v.dedup();
        v
    }
}

fn lvl(s: &J) -> u32 {
    s.get("level").and_then(|x| x.as_u64()).unwrap_or(0) as u32
}
fn num(s: &J, k: &str) -> Option<f64> {
    s.get(k).and_then(|x| x.as_f64())
}

/// Deliberate differences: class -> spell id -> field -> reason.
#[derive(Default)]
pub struct Accepted(HashMap<(String, String, String), String>);

impl Accepted {
    pub fn parse_all(texts: &[String]) -> Result<Accepted, String> {
        let mut all = Accepted::default();
        for t in texts {
            all.0.extend(Accepted::parse(t)?.0);
        }
        Ok(all)
    }
    pub fn parse(text: &str) -> Result<Accepted, String> {
        let v: toml::Table = toml::from_str(text).map_err(|e| format!("audit-accepted: {e}"))?;
        let mut m = HashMap::new();
        for (class, spells) in &v {
            for (spell, fields) in spells.as_table().into_iter().flatten() {
                for (field, why) in fields.as_table().into_iter().flatten() {
                    Accepted::insert(&mut m, class, spell, field, why);
                }
            }
        }
        Ok(Accepted(m))
    }
    /// An unquoted `value.0 = ".."` is a TOML dotted key, which parses as a
    /// table; it means the same as the quoted `"value.0"`.
    fn insert(m: &mut HashMap<(String, String, String), String>, class: &str, spell: &str, field: &str, why: &toml::Value) {
        match why {
            toml::Value::Table(t) => {
                for (k, v) in t {
                    Accepted::insert(m, class, spell, &format!("{field}.{k}"), v);
                }
            }
            _ => {
                m.insert((class.to_string(), spell.to_string(), field.to_string()), why.as_str().unwrap_or("").to_string());
            }
        }
    }
    fn why(&self, class: &str, spell: &str, field: &str) -> Option<&String> {
        self.0.get(&(class.to_string(), spell.to_string(), field.to_string())).or_else(|| self.0.get(&(class.to_string(), spell.to_string(), "*".to_string())))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Mismatch,
    Accepted,
    /// The game has no spell matching this rank.
    NoMatch,
}

#[derive(Debug, serde::Serialize)]
pub struct Row {
    pub class: String,
    pub spell: String,
    pub level: u32,
    pub game_id: Option<u64>,
    pub field: String,
    pub sim: Option<f64>,
    pub game: Option<f64>,
    pub status: Status,
    pub note: String,
}

/// Values for formulas, as far as they can be known without a fight.
struct AuditEnv<'a> {
    rank: &'a RankDef,
    opts: &'a [f64],
    level: u32,
    base: [f64; 7],
}

impl Env for AuditEnv<'_> {
    fn var(&self, v: &VarRef) -> f64 {
        match v {
            VarRef::Rank(i) => self.rank.value(*i, self.level).unwrap_or(f64::NAN),
            VarRef::Opt(i) => self.opts.get(*i as usize).copied().unwrap_or(f64::NAN),
            VarRef::Talent(..) => 0.0,
            VarRef::Level(_) => self.level as f64,
            VarRef::BaseMana(_) => self.base[6],
            VarRef::BaseHp(_) => self.base[5],
            _ => f64::NAN,
        }
    }
}

/// Values for a talent's formulas at one rank: the talent at that rank,
/// other talents untaken, default assumptions; anything else is unknown.
struct TalentEnv<'a> {
    talent: u16,
    rank: f64,
    opts: &'a [f64],
}

impl Env for TalentEnv<'_> {
    fn var(&self, v: &VarRef) -> f64 {
        match v {
            VarRef::Talent(_, i) => {
                if *i == self.talent {
                    self.rank
                } else {
                    0.0
                }
            }
            VarRef::Opt(i) => self.opts.get(*i as usize).copied().unwrap_or(f64::NAN),
            _ => f64::NAN,
        }
    }
}

/// How a game value may appear in the simulator: as is, as a fraction
/// (percent / 100), as a multiplier (1 +/- percent), in seconds (from ms),
/// in whole rage (the tables store tenths), with the sign flipped.
const SCALES: [fn(f64) -> f64; 10] = [
    |x| x,
    |x| x / 100.0,
    |x| 1.0 + x / 100.0,
    |x| 1.0 - x / 100.0,
    |x| x / 1000.0,
    |x| x / 10.0,
    |x| -x,
    |x| -x / 100.0,
    |x| -x / 1000.0,
    |x| -x / 10.0,
];

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6_f64.max(b.abs() * 2e-3)
}

fn ev(e: &Expr, env: &AuditEnv) -> Option<f64> {
    let x = e.eval(env);
    x.is_finite().then_some(x)
}

/// The game's damage or healing range for an effect for a caster of
/// `level`: base plus per-level growth (up to the rank's top level), spread
/// by the variance.
fn game_range(fx: &J, s: &J, level: u32) -> Option<(f64, f64)> {
    let base = num(fx, "base")?;
    let var = num(fx, "variance").unwrap_or(0.0);
    let top = num(s, "maxLevel").map(|m| m as u32).filter(|&m| m > 0).unwrap_or(60).max(lvl(s));
    let at = base + num(fx, "perLevel").unwrap_or(0.0) * (level.min(top).saturating_sub(lvl(s))) as f64;
    Some((at * (1.0 - var / 2.0), at * (1.0 + var / 2.0)))
}

fn effects_of(s: &J, kinds: &[&str]) -> Vec<J> {
    s.get("effects")
        .and_then(|e| e.as_array())
        .map(|a| a.iter().filter(|fx| fx.get("type").and_then(|t| t.as_str()).map_or(false, |t| kinds.contains(&t))).cloned().collect())
        .unwrap_or_default()
}

/// Direct damage and healing effects of a spell (not nested in conditions).
fn direct(effects: &[Effect]) -> (Vec<&DamageFx>, Vec<&HealFx>) {
    let mut d = Vec::new();
    let mut h = Vec::new();
    for e in effects {
        match e {
            Effect::Damage(x) if !x.periodic && x.weapon.is_none() => d.push(&**x),
            Effect::Heal(x) if !x.periodic => h.push(&**x),
            _ => {}
        }
    }
    (d, h)
}

pub struct Audit<'a> {
    pub w: &'a World,
    pub book: &'a Book,
    pub accepted: &'a Accepted,
    pub rows: Vec<Row>,
}

impl<'a> Audit<'a> {
    pub fn run(w: &'a World, book: &'a Book, accepted: &'a Accepted, only: Option<&str>) -> Audit<'a> {
        let mut a = Audit { w, book, accepted, rows: Vec::new() };
        let opts: Vec<f64> = w.opts.iter().map(|o| o.default).collect();
        for sp in &w.spells {
            let Some(ci) = sp.class else { continue };
            let class = &w.classes[ci];
            if only.map_or(false, |o| o != class.id) {
                continue;
            }
            let id_field = w.rank_fields.iter().position(|f| f == "id").map(|i| i as u16);
            let ranks: Vec<RankDef> = if sp.ranks.is_empty() {
                vec![RankDef { level: sp.learn_level, fields: vec![], grow: vec![], max_level: None }]
            } else {
                sp.ranks.clone()
            };
            let game_ranks = book.ranks(&class.id, &sp.name);
            for (ri, r) in ranks.iter().enumerate() {
                let env = AuditEnv { rank: r, opts: &opts, level: r.level, base: class.base_at(r.level) };
                let by_id = id_field.and_then(|f| r.fields.iter().find(|(k, _)| *k == f)).map(|(_, v)| *v as u64);
                // by id, then by level, then by rank order (talent spells are
                // listed at their talent tier or level 0 in the game)
                let game = match by_id {
                    Some(id) => book.get(id),
                    None => book.find(&class.id, &sp.name, r.level).or_else(|| {
                        if game_ranks.len() == ranks.len() {
                            game_ranks.get(ri).copied()
                        } else if game_ranks.len() == 1 && ranks.len() == 1 {
                            game_ranks.first().copied()
                        } else {
                            None
                        }
                    }),
                };
                let Some(g) = game else {
                    let lv = book.levels(&class.id, &sp.name);
                    let note = if lv.is_empty() { "no spell of this name in the game".into() } else { format!("the game has it at levels {lv:?}") };
                    a.push(&class.id, sp, r.level, None, "match", None, None, Status::NoMatch, note);
                    continue;
                };
                let gid = num(g, "id").map(|x| x as u64);
                a.check_spell(&class.id, class, sp, r, &env, g, gid);
            }
        }
        a
    }

    #[allow(clippy::too_many_arguments)]
    fn push(&mut self, class: &str, sp: &SpellDef, level: u32, gid: Option<u64>, field: &str, sim: Option<f64>, game: Option<f64>, status: Status, note: String) {
        let (status, note) = match (status, self.accepted.why(class, &sp.id, field)) {
            (Status::Mismatch | Status::NoMatch, Some(why)) => (Status::Accepted, why.clone()),
            (s, _) => (s, note),
        };
        self.rows.push(Row { class: class.into(), spell: sp.id.clone(), level, game_id: gid, field: field.into(), sim, game, status, note });
    }

    #[allow(clippy::too_many_arguments)]
    fn compare(&mut self, class: &str, sp: &SpellDef, level: u32, gid: Option<u64>, field: &str, sim: Option<f64>, game: Option<f64>, tol: f64) {
        let (Some(s), Some(g)) = (sim, game) else { return };
        let ok = (s - g).abs() <= tol.max(g.abs() * 1e-3);
        self.push(class, sp, level, gid, field, Some(s), Some(g), if ok { Status::Ok } else { Status::Mismatch }, String::new());
    }

    #[allow(clippy::too_many_arguments)]
    fn check_spell(&mut self, cid: &str, class: &ClassDef, sp: &SpellDef, r: &RankDef, env: &AuditEnv, g: &J, gid: Option<u64>) {
        let lv = r.level;
        // level the rank is learned at
        if lvl(g) > 0 {
            self.compare(cid, sp, lv, gid, "level", Some(lv as f64), Some(lvl(g) as f64), 0.0);
        }
        // the icon the website shows, from the spell's first rank
        let first = sp.ranks.first().map_or(true, |f| f.level == r.level);
        if let (true, Some(game_icon)) = (first, g.get("icon").and_then(|x| x.as_str())) {
            let sim_icon = sp.icon.as_deref().unwrap_or("");
            let ok = sim_icon == game_icon;
            let note = if ok { String::new() } else { format!("icon {sim_icon:?}, the game has {game_icon:?} (tools/spell_icons.py sets it)") };
            self.push(cid, sp, lv, gid, "icon", None, None, if ok { Status::Ok } else { Status::Mismatch }, note);
        }
        // the school (it decides what Silence and an interrupt's lockout stop)
        if let (true, Some(mask)) = (first, g.get("school").and_then(|x| x.as_u64())) {
            let bit = match sp.school {
                crate::vars::School::Physical => 1, crate::vars::School::Holy => 2, crate::vars::School::Fire => 4, crate::vars::School::Nature => 8,
                crate::vars::School::Frost => 16, crate::vars::School::Shadow => 32, crate::vars::School::Arcane => 64,
            };
            let ok = mask & bit != 0;
            let note = if ok { String::new() } else { format!("school {}, the game's mask is {mask} (1 physical, 2 holy, 4 fire, 8 nature, 16 frost, 32 shadow, 64 arcane)", sp.school.name()) };
            self.push(cid, sp, lv, gid, "school", Some(bit as f64), Some(mask as f64), if ok { Status::Ok } else { Status::Mismatch }, note);
        }
        // the dispel type of the debuffs it leaves (what cures take them off)
        let mut auras = vec![];
        crate::world::applied_auras(&sp.effects, &mut auras);
        if let Some(ch) = &sp.channel {
            crate::world::applied_auras(&ch.effects, &mut auras);
        }
        let debuffs = auras.iter().any(|&a| { let d = &self.w.auras[a as usize]; d.debuff && d.dispel.is_none() });
        if first && debuffs {
            let game = g.get("dispel").and_then(|x| x.as_u64()).unwrap_or(0);
            let want = match game { 1 => DISPEL_MAGIC, 2 => DISPEL_CURSE, 3 => DISPEL_DISEASE, 4 => DISPEL_POISON, _ => 0 };
            let ok = sp.dispel == want;
            let note = if ok { String::new() } else { format!("dispel type {}, the game's is {game} (1 magic, 2 curse, 3 disease, 4 poison)", sp.dispel) };
            self.push(cid, sp, lv, gid, "dispel", Some(sp.dispel as f64), Some(want as f64), if ok { Status::Ok } else { Status::Mismatch }, note);
        }
        // costs
        let game_cost = |power: &str| -> Option<f64> {
            let c = g.get("cost")?.as_array()?.iter().find(|c| c.get("power").and_then(|p| p.as_str()) == Some(power))?;
            let amount = num(c, "amount").unwrap_or(0.0) + num(c, "pctOfBase").unwrap_or(0.0) / 100.0 * env.base[6];
            // rage is stored in tenths
            Some(if power == "rage" { amount / 10.0 } else { amount })
        };
        for (res, name) in [(Res::Mana, "mana"), (Res::Rage, "rage"), (Res::Energy, "energy")] {
            let sim = sp.cost.iter().find(|(r, _)| *r == res).and_then(|(_, e)| ev(e, env));
            let game = game_cost(name);
            if sim.is_some() || game.is_some() {
                self.compare(cid, sp, lv, gid, &format!("cost.{name}"), Some(sim.unwrap_or(0.0)), Some(game.unwrap_or(0.0)), 0.5);
            }
        }
        // cast time (a channel's length is its duration in the game)
        match &sp.channel {
            Some(ch) => self.compare(cid, sp, lv, gid, "channel", ev(&ch.duration, env), Some(num(g, "duration").unwrap_or(0.0)), 0.01),
            None => self.compare(cid, sp, lv, gid, "cast", ev(&sp.cast_time, env), Some(num(g, "cast").unwrap_or(0.0).max(0.0)), 0.01),
        }
        // cooldown: the longer of the spell's own and its category's
        let gcd_game = num(g, "cooldown").unwrap_or(0.0).max(num(g, "categoryCooldown").unwrap_or(0.0));
        self.compare(cid, sp, lv, gid, "cooldown", ev(&sp.cooldown, env), Some(gcd_game), 0.01);
        // global cooldown
        if let Some(gg) = num(g, "gcd") {
            let def = if class.energy_gcd { self.w.combat.energy_gcd } else { self.w.combat.gcd };
            let sim = if sp.off_gcd || sp.next_swing { Some(0.0) } else { sp.gcd.as_ref().map_or(Some(def), |e| ev(e, env)) };
            self.compare(cid, sp, lv, gid, "gcd", sim, Some(gg), 0.01);
        }
        // direct damage and healing: range when learned and at the top of the
        // rank (the game raises it with the caster's level), coefficient
        let top = num(g, "maxLevel").map(|m| m as u32).filter(|&m| m > 0).unwrap_or(60).max(lv);
        let env_top = AuditEnv { rank: env.rank, opts: env.opts, level: top, base: env.base };
        let (dmg, heal) = direct(&sp.effects);
        let checks: [(&str, Option<(&Option<Expr>, &Option<Expr>, &Option<Expr>)>, Vec<J>); 2] = [
            ("damage", dmg.first().map(|d| (&d.min, &d.max, &d.coef)), effects_of(g, &["school_damage"])),
            ("heal", heal.first().map(|h| (&h.min, &h.max, &h.coef)), effects_of(g, &["heal"])),
        ];
        for (what, sim, gfx) in checks {
            let (Some((min, max, coef)), Some(gfx)) = (sim, gfx.first()) else { continue };
            if let (Some(min), Some(max)) = (min, max) {
                if let (Some((glo, ghi)), Some((tlo, thi))) = (game_range(gfx, g, lv), game_range(gfx, g, top)) {
                    self.compare(cid, sp, lv, gid, &format!("{what}.min"), ev(min, env), Some(glo.round()), 1.5);
                    self.compare(cid, sp, lv, gid, &format!("{what}.max"), ev(max, env), Some(ghi.round()), 1.5);
                    if top > lv && num(gfx, "perLevel").is_some() {
                        self.compare(cid, sp, lv, gid, &format!("{what}.min@{top}"), ev(min, &env_top), Some(tlo.round()), 1.5);
                        self.compare(cid, sp, lv, gid, &format!("{what}.max@{top}"), ev(max, &env_top), Some(thi.round()), 1.5);
                    }
                }
            }
            if let Some(c) = coef {
                self.compare(cid, sp, lv, gid, &format!("{what}.coef"), ev(c, env), Some(num(gfx, "coef").unwrap_or(0.0)), 0.005);
            }
        }
        // how far it reaches: the game's range (and the closest), and an
        // area's radius (an area around you has no range of its own)
        let grad = g.get("effects").and_then(|x| x.as_array()).map_or(0.0, |a| a.iter().filter_map(|e| num(e, "radius")).fold(0.0f64, f64::max));
        let grange = g.get("range").and_then(|x| x.as_array()).map(|a| (a.first().and_then(|v| v.as_f64()).unwrap_or(0.0), a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0)));
        if sp.radius > 0.0 || grad > 0.0 {
            self.compare(cid, sp, lv, gid, "radius", Some(sp.radius), Some(grad), 0.01);
        }
        match grange {
            Some((lo, hi)) if hi > 0.0 => {
                self.compare(cid, sp, lv, gid, "range", Some(sp.range), Some(hi), 0.01);
                self.compare(cid, sp, lv, gid, "min_range", Some(sp.min_range), Some(lo), 0.01);
            }
            _ if sp.radius > 0.0 && sp.range <= sp.radius + 1e-9 => {}
            _ => self.compare(cid, sp, lv, gid, "range", Some(sp.range), Some(0.0), 0.01),
        }
        // how many enemies an area or chain effect reaches (0 in the game = no cap)
        if let Some(d) = dmg.iter().find(|d| matches!(d.targets, TargetSel::Enemies(_) | TargetSel::Chain { .. })) {
            if let TargetSel::Enemies(max) | TargetSel::Chain { max, .. } = &d.targets {
                let chain = effects_of(g, &["school_damage", "weapon_damage", "normalized_weapon_damage", "weapon_percent_damage"])
                    .iter()
                    .filter_map(|fx| num(fx, "chain"))
                    .fold(0.0f64, f64::max);
                let cap = num(g, "maxTargets").unwrap_or(0.0).max(chain);
                let sim = ev(max, env).map(|x| if x >= 100.0 { 0.0 } else { x });
                self.compare(cid, sp, lv, gid, "targets", sim, Some(cap), 0.0);
            }
        }
        // auras the spell applies: duration
        if let Some(gdur) = num(g, "duration") {
            for e in &sp.effects {
                if let Effect::ApplyAura { aura, duration, .. } = e {
                    let au = &self.w.auras[*aura as usize];
                    let sim = duration.as_ref().map_or_else(|| ev(&au.duration, env), |d| ev(d, env));
                    // buffs of 10 minutes or more (or permanent) count as permanent
                    let long = |x: f64| x <= 0.0 || x >= 600.0;
                    let game = if long(gdur) { 0.0 } else { gdur };
                    let sim = sim.map(|x| if long(x) || x >= 1e8 { 0.0 } else { x });
                    self.compare(cid, sp, lv, gid, &format!("duration.{}", au.id), sim, Some(game), 0.01);
                    break;
                }
            }
        }
    }

    /// Talents: are they in the game's tree with the same ranks, the same
    /// points needed and the same prerequisite, and do the game's per-rank
    /// values appear among the numbers the talent drives in the simulator?
    pub fn run_talents(&mut self, only: Option<&str>) {
        let w = self.w;
        for (ci, class) in w.classes.iter().enumerate() {
            if only.map_or(false, |o| o != class.id) {
                continue;
            }
            let game = self.book.talents.get(&class.id).cloned().unwrap_or_default();
            if game.is_empty() {
                continue;
            }
            // every formula of the class, by the talents it reads
            let mut by_talent: HashMap<u16, Vec<&Expr>> = HashMap::new();
            crate::validate::class_exprs(w, ci, false, &mut |e| {
                for t in crate::validate::talents_in(e) {
                    by_talent.entry(t).or_default().push(e);
                }
            });
            let opts: Vec<f64> = w.opts.iter().map(|o| o.default).collect();
            let mut seen = vec![false; game.len()];
            for (ti, t) in class.talents.iter().enumerate() {
                let key = format!("talent_{}", t.id);
                let Some(gi) = game.iter().position(|g| norm(&g.name) == norm(&t.name)) else {
                    self.push_t(&class.id, &key, "talent", None, None, Status::NoMatch, "not in the game's talent tree".into());
                    continue;
                };
                seen[gi] = true;
                let g = &game[gi];
                let gid = Some(g.spell);
                self.compare_t(&class.id, &key, gid, "ranks", t.ranks as f64, g.ranks as f64);
                self.compare_t(&class.id, &key, gid, "points", t.points as f64, g.points as f64);
                let sim_req = t.requires.map(|r| class.talents[r as usize].name.clone());
                let ok = match &sim_req {
                    None => g.requires.is_empty(),
                    Some(n) => g.requires.iter().any(|x| norm(x) == norm(n)),
                };
                let note = format!("simulator: {}; game: {}", sim_req.as_deref().unwrap_or("none"), if g.requires.is_empty() { "none".into() } else { g.requires.join(", ") });
                self.push_t(&class.id, &key, "requires", None, None, if ok { Status::Ok } else { Status::Mismatch }, if ok { String::new() } else { note });
                // values: every number the talent drives, at each rank
                let exprs = by_talent.get(&(ti as u16)).cloned().unwrap_or_default();
                let sim_at = |r: u32| -> Vec<f64> {
                    let env = TalentEnv { talent: ti as u16, rank: r as f64, opts: &opts };
                    exprs.iter().map(|e| e.eval(&env)).filter(|x| x.is_finite()).collect()
                };
                for (eff, ranks) in &g.values {
                    if ranks.iter().all(|x| x.abs() < 1e-9) || ranks.len() != t.ranks as usize {
                        continue;
                    }
                    let found = SCALES.iter().any(|f| ranks.iter().enumerate().all(|(i, &gv)| sim_at(i as u32 + 1).iter().any(|&sv| close(sv, f(gv)))));
                    let shown: Vec<String> = ranks.iter().map(|x| format!("{x}")).collect();
                    let top = sim_at(t.ranks);
                    let mut sims: Vec<String> = top.iter().map(|x| format!("{:.4}", x).trim_end_matches('0').trim_end_matches('.').to_string()).collect();
                    sims.sort();
                    sims.dedup();
                    let note = if found {
                        String::new()
                    } else {
                        format!("game per rank [{}] not found; simulator's numbers at rank {}: [{}]", shown.join(", "), t.ranks, sims.join(", "))
                    };
                    self.push_t(&class.id, &key, &format!("value.{eff}"), Some(ranks[ranks.len() - 1]), None, if found { Status::Ok } else { Status::Mismatch }, note);
                }
            }
            for (gi, g) in game.iter().enumerate() {
                if !seen[gi] {
                    self.push_t(&class.id, &format!("talent_{}", norm(&g.name)), "talent", None, None, Status::NoMatch, format!("{} is in the game's tree but not in the simulator", g.name));
                }
            }
        }
    }

    fn push_t(&mut self, class: &str, key: &str, field: &str, game: Option<f64>, sim: Option<f64>, status: Status, note: String) {
        let (status, note) = match (status, self.accepted.why(class, key, field)) {
            (Status::Mismatch | Status::NoMatch, Some(why)) => (Status::Accepted, why.clone()),
            (s, _) => (s, note),
        };
        self.rows.push(Row { class: class.into(), spell: key.into(), level: 0, game_id: None, field: field.into(), sim, game, status, note });
    }

    fn compare_t(&mut self, class: &str, key: &str, _gid: Option<u64>, field: &str, sim: f64, game: f64) {
        let st = if (sim - game).abs() < 1e-9 { Status::Ok } else { Status::Mismatch };
        self.push_t(class, key, field, Some(game), Some(sim), st, String::new());
    }

    pub fn failures(&self) -> usize {
        self.rows.iter().filter(|r| matches!(r.status, Status::Mismatch | Status::NoMatch)).count()
    }

    /// Plain-text report: a summary per class, then every problem.
    pub fn report(&self, all: bool) -> String {
        let mut out = format!("Checked against WoW: Forever spell data, build {}\n\n", self.book.build);
        let mut classes: Vec<&str> = self.rows.iter().map(|r| r.class.as_str()).collect();
        classes.dedup();
        classes.sort();
        classes.dedup();
        out.push_str(&format!("{:<8} {:>6} {:>9} {:>9} {:>9}\n", "class", "ok", "mismatch", "no match", "accepted"));
        for c in &classes {
            let n = |s: Status| self.rows.iter().filter(|r| r.class == *c && r.status == s).count();
            out.push_str(&format!("{:<8} {:>6} {:>9} {:>9} {:>9}\n", c, n(Status::Ok), n(Status::Mismatch), n(Status::NoMatch), n(Status::Accepted)));
        }
        let fmt = |x: Option<f64>| x.map_or("-".to_string(), |v| if (v - v.round()).abs() < 1e-9 { format!("{v:.0}") } else { format!("{v:.3}") });
        for c in &classes {
            let rows: Vec<&Row> = self.rows.iter().filter(|r| r.class == *c && (all || r.status != Status::Ok)).collect();
            if rows.is_empty() {
                continue;
            }
            out.push_str(&format!("\n== {c}\n"));
            for r in rows {
                let tag = match r.status {
                    Status::Ok => "ok      ",
                    Status::Mismatch => "MISMATCH",
                    Status::NoMatch => "NO MATCH",
                    Status::Accepted => "accepted",
                };
                let id = r.game_id.map_or(String::new(), |i| format!(" #{i}"));
                out.push_str(&format!("  {tag} {:<26} lvl {:>2}{:<9} {:<26} sim {:>9}  game {:>9}  {}\n", r.spell, r.level, id, r.field, fmt(r.sim), fmt(r.game), r.note));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_grow_with_level_up_to_their_top() {
        let r = RankDef { level: 50, fields: vec![(0, 100.0), (1, 7.0)], grow: vec![(0, 2.0)], max_level: Some(55) };
        assert_eq!(r.value(0, 50), Some(100.0));
        assert_eq!(r.value(0, 53), Some(106.0));
        assert_eq!(r.value(0, 60), Some(110.0)); // stops at max_level
        assert_eq!(r.value(0, 40), Some(100.0)); // never below the learned value
        assert_eq!(r.value(1, 60), Some(7.0)); // fields without growth stay put
        assert_eq!(r.value(2, 60), None);
    }

    #[test]
    fn spells_match_by_level_and_prefer_castable_copies() {
        let book = Book::parse(r#"{"build": "x", "spells": {
            "1": {"id": 1, "name": "Bolt", "class": "shaman", "skill": [1], "level": 10, "cost": [{"power": "mana", "amount": 5}]},
            "2": {"id": 2, "name": "Bolt", "class": "shaman", "skill": [1], "level": 10},
            "3": {"id": 3, "name": "Bolt", "class": "shaman", "skill": [1], "level": 20, "cast": 2.0}
        }}"#).unwrap();
        assert_eq!(book.find("shaman", "bolt", 10).and_then(|s| num(s, "id")), Some(1.0));
        assert_eq!(book.find("shaman", "Bolt", 20).and_then(|s| num(s, "id")), Some(3.0));
        assert!(book.find("mage", "Bolt", 10).is_none());
        assert_eq!(book.ranks("shaman", "Bolt").len(), 2);
    }

    #[test]
    fn game_ranges_spread_by_variance_and_grow_to_the_top_level() {
        let s: J = serde_json::from_str(r#"{"level": 56, "maxLevel": 61}"#).unwrap();
        let fx: J = serde_json::from_str(r#"{"base": 196, "variance": 0.108, "perLevel": 1.2}"#).unwrap();
        let (lo, hi) = game_range(&fx, &s, 56).unwrap();
        assert_eq!((lo.round(), hi.round()), (185.0, 207.0));
        let (lo, hi) = game_range(&fx, &s, 60).unwrap();
        assert_eq!((lo.round(), hi.round()), (190.0, 212.0)); // Lightning Bolt rank 10 at 60, as the tooltip shows
    }

    #[test]
    fn accepted_fields_read_the_same_quoted_or_dotted() {
        let a = Accepted::parse("[priest.talent_x]\n\"value.0\" = \"quoted\"\nvalue.1 = \"dotted\"\n[priest.y]\n\"*\" = \"all\"\n").unwrap();
        assert_eq!(a.why("priest", "talent_x", "value.0").map(|s| s.as_str()), Some("quoted"));
        assert_eq!(a.why("priest", "talent_x", "value.1").map(|s| s.as_str()), Some("dotted"));
        assert_eq!(a.why("priest", "y", "cast").map(|s| s.as_str()), Some("all"));
        assert!(a.why("priest", "talent_x", "value.2").is_none());
    }
}
