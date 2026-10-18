//! Rotation conditions in plain English, for the website's "how this build
//! played" list: `target.hp_pct < 0.2` reads "the target is below 20%
//! health". Common patterns get a phrase; a condition with any part that
//! has none is shown as written instead, so nothing is left out.
use crate::expr::{Expr, Node};
use crate::model::*;
use crate::vars::*;

/// The condition as a phrase ("your health is below 35%"), or the formula
/// itself when some part has no phrase. None when it only waits for the
/// global cooldown (the line is used whenever nothing above it is).
pub fn condition(w: &World, class: Option<usize>, e: &Expr) -> Option<String> {
    let x = Explain { w, class };
    if let Some(Part::Skip) = x.part(&e.node) {
        return None;
    }
    Some(x.node(&e.node).unwrap_or_else(|| e.src.clone())).filter(|t| !t.is_empty())
}

struct Explain<'a> {
    w: &'a World,
    class: Option<usize>,
}

fn num(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 { format!("{}", x.round() as i64) } else { format!("{x:.1}") }
}

fn pct(x: f64) -> String {
    num(x * 100.0) + "%"
}

fn humanize(id: &str) -> String {
    let s = id.replace('_', " ");
    s.split(' ').map(|w| {
        let mut c = w.chars();
        c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
    }).collect::<Vec<_>>().join(" ")
}

/// `x op c` with the constant on the right; flips `c op x`.
fn cmp_parts<'n>(op: &'static str, a: &'n Node, b: &'n Node) -> Option<(&'static str, &'n Node, f64)> {
    match (a, b) {
        (x, Node::C(c)) => Some((op, x, *c)),
        (Node::C(c), x) => Some((flip(op), x, *c)),
        (x, Node::Neg(n)) => match n.as_ref() { Node::C(c) => Some((op, x, -c)), _ => None },
        _ => None,
    }
}

fn flip(op: &'static str) -> &'static str {
    match op { "<" => ">", ">" => "<", "<=" => ">=", ">=" => "<=", o => o }
}

impl Explain<'_> {
    /// Stances and forms read "you're in Battle Stance".
    fn stance(&self, id: u32) -> bool {
        self.w.auras.get(id as usize).map_or(false, |a| a.form.is_some() || a.group.map_or(false, |g| self.w.groups.get(g as usize).map_or(false, |n| n == "stance")))
    }
    /// "Overpower window" and the like: a short chance to use something.
    fn window(&self, id: u32) -> bool {
        self.w.auras.get(id as usize).map_or(false, |a| a.name.ends_with(" window"))
    }
    fn aura(&self, id: u32) -> String {
        self.w.auras.get(id as usize).map_or_else(|| "a buff".into(), |a| a.name.clone())
    }
    fn spell(&self, id: u32) -> String {
        self.w.spells.get(id as usize).map_or_else(|| "a spell".into(), |s| s.name.clone())
    }
    fn talent(&self, t: u16) -> String {
        self.class.and_then(|c| self.w.classes[c].talents.get(t as usize)).map_or_else(|| "the talent".into(), |t| t.name.clone())
    }
    fn whose(who: Who) -> &'static str {
        match who {
            Who::Me => "your",
            Who::Target => "the target's",
            Who::Pet => "your pet's",
            Who::Owner => "your master's",
            // the ally a heal line is on: "their health is below 40%"
            Who::Ally => "their",
            Who::Tank => "the tank's",
            Who::Lowest => "the most hurt ally's",
        }
    }
    fn subject(who: Who) -> &'static str {
        match who {
            Who::Me => "you",
            Who::Target => "the target",
            Who::Pet => "your pet",
            Who::Owner => "your master",
            Who::Ally => "they",
            Who::Tank => "the tank",
            Who::Lowest => "the most hurt ally",
        }
    }

    /// The node as a true-or-false phrase.
    fn node(&self, n: &Node) -> Option<String> {
        match n {
            Node::And(a, b) => {
                // the global cooldown check is how the engine waits; leave it out
                let (a, b) = (self.part(a), self.part(b));
                match (a, b) {
                    (Some(Part::Skip), Some(Part::Skip)) => Some(String::new()),
                    (Some(Part::Skip), Some(Part::Text(t))) | (Some(Part::Text(t)), Some(Part::Skip)) => Some(t),
                    // "x, unless ..." reads better than "x and unless ..."
                    (Some(Part::Text(x)), Some(Part::Text(y))) if y.starts_with("unless ") => Some(format!("{x}, {y}")),
                    (Some(Part::Text(x)), Some(Part::Text(y))) => Some(format!("{x} and {y}")),
                    _ => None,
                }
            }
            Node::Or(..) => {
                // a chain of or's reads "a, b or c"
                let mut terms = vec![];
                or_terms(n, &mut terms);
                let words = terms.iter().map(|t| self.inner(t)).collect::<Option<Vec<_>>>()?;
                let (last, rest) = words.split_last()?;
                Some(if rest.is_empty() { last.clone() } else { format!("{} or {last}", rest.join(", ")) })
            }
            Node::Not(x) => self.not(x),
            Node::Cmp(op, a, b) => self.cmp(op, a, b),
            Node::V(v) => self.flag(v),
            // `c ? x : y` as a rule: "x when c, otherwise y"
            Node::Tern(c, x, y) => Some(format!("{} when {}, otherwise {}", self.inner(x)?, self.inner(c)?, self.inner(y)?)),
            _ => None,
        }
    }

    /// An `or` inside an `and` gets parentheses.
    fn inner(&self, n: &Node) -> Option<String> {
        let t = self.node(n)?;
        Some(if matches!(n, Node::Or(..) | Node::Tern(..)) { format!("({t})") } else { t })
    }

    fn part(&self, n: &Node) -> Option<Part> {
        if let Node::Cmp(_, a, _) = n {
            if matches!(a.as_ref(), Node::V(VarRef::GcdRemains)) {
                return Some(Part::Skip);
            }
        }
        let t = self.inner(n)?;
        Some(if t.is_empty() { Part::Skip } else { Part::Text(t) })
    }

    fn flag(&self, v: &VarRef) -> Option<String> {
        Some(match v {
            VarRef::AuraUp(Who::Me, a) if self.stance(*a) => format!("you're in {}", self.aura(*a)),
            VarRef::AuraUp(Who::Me, a) if self.window(*a) => format!("the {} is open", self.aura(*a)),
            VarRef::AuraUp(Who::Me, a) => format!("{} is up", self.aura(*a)),
            VarRef::AuraUp(Who::Ally, a) => format!("they have {}", self.aura(*a)),
            VarRef::AuraUp(who, a) => format!("{} has {}", Self::subject(*who), self.aura(*a)),
            VarRef::IsTank(Who::Ally) => "they're the tank".into(),
            VarRef::IsMe(Who::Ally) => "it's on you".into(),
            VarRef::Boss(Who::Target) => "the target is a boss".into(),
            VarRef::IsPlayer(Who::Target) => "the target is a player".into(),
            VarRef::Known(s) => format!("you know {}", self.spell(*s)),
            VarRef::CdReady(s) => format!("{} is ready", self.spell(*s)),
            VarRef::Mech(who, m) => format!("{} {} {}", Self::subject(*who), if matches!(*who, Who::Me | Who::Ally) { "are" } else { "is" }, mech(*m)),
            VarRef::Casting(who) => format!("{} {} casting", Self::subject(*who), if matches!(*who, Who::Me | Who::Ally) { "are" } else { "is" }),
            VarRef::Wanding(Who::Me) => "your wand is shooting".into(),
            VarRef::Form(f) => format!("you're in {}", humanize(self.w.forms.get(*f as usize).map_or("a form", |s| s.as_str()))),
            VarRef::InAnyForm => "you're in a form".into(),
            VarRef::PetActive(None) => "your pet is out".into(),
            VarRef::PetActive(Some(p)) => format!("your {} is out", self.w.pets.get(*p as usize).map_or("pet".into(), |d| d.name.clone())),
            VarRef::InCombat => "you're in combat".into(),
            VarRef::UnitInCombat(who) => format!("{} is in combat", Self::subject(*who)),
            VarRef::TotemActive(e) => format!("your {} is down", totem_word(*e)),
            VarRef::TotemInRange(e) => format!("your {} reaches", totem_word(*e)),
            VarRef::Fleeing(who) => format!("{} is running away", Self::subject(*who)),
            VarRef::Moving(Who::Me) => "you're moving".into(),
            VarRef::Behind => "you're behind the target".into(),
            VarRef::DualWield => "you wield two weapons".into(),
            VarRef::TwoHand => "you wield a two-handed weapon".into(),
            VarRef::HasShield => "you carry a shield".into(),
            VarRef::Creature(Who::Target, c) => format!("the target is {}", creature(*c)),
            VarRef::Talent(_, t) => format!("you have {}", self.talent(*t)),
            VarRef::WeaponFamily(h, wt) => format!("you wield {} in your {}", weapon(*wt), hand(*h)),
            VarRef::Race(r) => format!("you're {}", self.w.races.get(*r as usize).map_or("of that race".into(), |x| article(&x.name))),
            VarRef::Class(c) => format!("you're {}", self.w.classes.get(*c as usize).map_or("of that class".into(), |x| article(&x.name))),
            VarRef::Opt(i) => match self.w.opts.get(*i as usize) {
                Some(o) if o.name == "potions" => "potions are on".into(),
                Some(o) => format!("the assumption “{}” is on", o.label),
                None => return None,
            },
            _ => return None,
        })
    }

    fn not(&self, x: &Node) -> Option<String> {
        if let Some(t) = self.finishing(x) {
            return Some(t);
        }
        let Node::V(v) = x else {
            return Some(format!("not ({})", self.node(x)?));
        };
        Some(match v {
            VarRef::AuraUp(Who::Me, a) if self.stance(*a) => format!("you're not in {}", self.aura(*a)),
            VarRef::AuraUp(Who::Me, a) if self.window(*a) => format!("the {} isn't open", self.aura(*a)),
            VarRef::AuraUp(Who::Me, a) => format!("{} is not up", self.aura(*a)),
            VarRef::AuraUp(Who::Ally, a) => format!("they don't have {}", self.aura(*a)),
            VarRef::AuraUp(who, a) => format!("{} doesn't have {}", Self::subject(*who), self.aura(*a)),
            VarRef::IsTank(Who::Ally) => "they're not the tank".into(),
            VarRef::IsMe(Who::Ally) => "it's on someone else".into(),
            VarRef::Boss(Who::Target) => "the target isn't a boss".into(),
            VarRef::IsPlayer(Who::Target) => "the target isn't a player".into(),
            VarRef::Known(s) => format!("you don't know {} yet", self.spell(*s)),
            VarRef::CdReady(s) => format!("{} isn't ready", self.spell(*s)),
            VarRef::Mech(who, m) => format!("{} {} not {}", Self::subject(*who), if matches!(*who, Who::Me | Who::Ally) { "are" } else { "is" }, mech(*m)),
            VarRef::Casting(who) => format!("{} {} not casting", Self::subject(*who), if matches!(*who, Who::Me | Who::Ally) { "are" } else { "is" }),
            VarRef::Wanding(Who::Me) => "your wand isn't shooting".into(),
            VarRef::Form(f) => format!("you're not in {}", humanize(self.w.forms.get(*f as usize).map_or("a form", |s| s.as_str()))),
            VarRef::InAnyForm => "you're not in a form".into(),
            VarRef::PetActive(None) => "your pet isn't out".into(),
            VarRef::PetActive(Some(p)) => format!("your {} isn't out", self.w.pets.get(*p as usize).map_or("pet".into(), |d| d.name.clone())),
            VarRef::WeaponFamily(h, wt) => format!("you don't wield {} in your {}", weapon(*wt), hand(*h)),
            VarRef::UnitInCombat(who) => format!("{} hasn't noticed anyone yet", Self::subject(*who)),
            VarRef::TotemActive(e) => format!("you have no {} down", totem_word(*e)),
            VarRef::TotemInRange(e) => format!("your {} is out of reach", totem_word(*e)),
            VarRef::Fleeing(who) => format!("{} isn't running away", Self::subject(*who)),
            VarRef::Moving(Who::Me) => "you're standing still".into(),
            VarRef::Behind => "you're not behind the target".into(),
            VarRef::InCombat => "you're out of combat".into(),
            VarRef::Talent(_, t) => format!("you don't have {}", self.talent(*t)),
            _ => return None,
        })
    }

    /// The rule that holds back a self-heal at the end of a fight:
    /// `!(target low && your health not in danger)`, in any of its forms.
    fn finishing(&self, x: &Node) -> Option<String> {
        let mut terms = vec![];
        and_terms(x, &mut terms);
        if terms.len() < 2 {
            return None;
        }
        let mut target = None;
        let mut safe = false;
        let mut mob = false;
        for t in &terms {
            if matches!(t, Node::Not(x) if matches!(x.as_ref(), Node::V(VarRef::IsPlayer(Who::Target)))) {
                mob = true;
                continue;
            }
            let Node::Cmp(op, a, b) = t else { continue };
            match (op.as_ref(), a.as_ref(), b.as_ref()) {
                ("<", Node::V(VarRef::ResPct(Who::Target, Res::Hp)), Node::C(c)) => target = Some(format!("is below {}", pct(*c))),
                ("<", Node::V(VarRef::Res(Who::Target, Res::Hp)), _) => target = Some("is nearly dead".to_string()),
                (">", Node::V(VarRef::ResPct(Who::Me, Res::Hp)), _) => safe = true,
                _ => {}
            }
        }
        let target = target?;
        let who = if mob { "the mob" } else { "the target" };
        safe.then(|| format!("unless {who} {target} and you're not in danger"))
    }

    fn cmp(&self, op: &'static str, a: &Node, b: &Node) -> Option<String> {
        // `x >= (c ? 5 : 3)`: "x is at least 5 when c, otherwise at least 3"
        if let Node::Tern(c, y, z) = b {
            if let (Node::C(_), Node::C(_)) = (y.as_ref(), z.as_ref()) {
                return Some(format!("{} when {}, otherwise {}", self.cmp(op, a, y)?, self.inner(c)?, self.cmp(op, a, z)?));
            }
        }
        // `target.hp < <the hit's damage>`: the hit would finish it
        if let (Node::V(VarRef::Res(Who::Target, Res::Hp)), "<" | "<=") = (a, op) {
            if cmp_parts(op, a, b).is_none() {
                return Some("it would finish the target".into());
            }
        }
        let (op, x, c) = cmp_parts(op, a, b)?;
        let Node::V(v) = x else { return None };
        let rel = |more: &str, less: &str| -> &'static str {
            match op {
                ">" => if more == "above" { "above" } else { "more than" },
                ">=" => "at least",
                "<" => if less == "below" { "below" } else { "less than" },
                "<=" => "at most",
                "==" => "exactly",
                _ => "not",
            }
        };
        Some(match v {
            VarRef::ResPct(who, Res::Hp) => format!("{} health is {} {}", Self::whose(*who), rel("above", "below"), pct(c)),
            VarRef::ResPct(who, Res::Mana) => format!("{} mana is {} {}", Self::whose(*who), rel("above", "below"), pct(c)),
            VarRef::ResPct(who, r) => format!("{} {} is {} {}", Self::whose(*who), r.name(), rel("above", "below"), pct(c)),
            VarRef::Res(Who::Me, Res::Rage) => format!("you have {} {} Rage", rel("more", "less"), num(c)),
            VarRef::Res(Who::Me, Res::Energy) => format!("you have {} {} Energy", rel("more", "less"), num(c)),
            VarRef::Res(who, r) => format!("{} {} is {} {}", Self::whose(*who), r.name(), rel("above", "below"), num(c)),
            VarRef::ResMax(Who::Target, Res::Hp) => format!("the target's maximum health is {} {}", rel("above", "below"), num(c)),
            VarRef::Weapon(h, WField::SwingRemains) => format!("your next {} swing is {} {} sec away", hand(*h), rel("more", "less"), num(c)),
            VarRef::Combo => match op {
                ">=" => format!("you have {} or more combo points", num(c)),
                _ => format!("you have {} {} combo points", rel("more", "fewer"), num(c)),
            },
            VarRef::AuraStacks(who, a) => {
                let on = if *who == Who::Me { String::new() } else { format!(" on {}", if *who == Who::Ally { "them" } else { Self::subject(*who) }) };
                match op {
                    "<" => format!("{}{} has fewer than {} stacks", self.aura(*a), on, num(c)),
                    _ => format!("{}{} has {} {} stacks", self.aura(*a), on, rel("more", "fewer"), num(c)),
                }
            }
            VarRef::AuraRemains(who, a) => {
                let on = if *who == Who::Me { String::new() } else { format!(" on {}", if *who == Who::Ally { "them" } else { Self::subject(*who) }) };
                format!("{}{} has {} {} sec left", self.aura(*a), on, rel("more", "less"), num(c))
            }
            VarRef::AuraCharges(who, a) => {
                let on = if *who == Who::Me { String::new() } else { format!(" on {}", if *who == Who::Ally { "them" } else { Self::subject(*who) }) };
                match op {
                    "<" => format!("{}{} has fewer than {} charges", self.aura(*a), on, num(c)),
                    _ => format!("{}{} has {} {} charges", self.aura(*a), on, rel("more", "fewer"), num(c)),
                }
            }
            VarRef::Opt(i) if (op == ">" && c == 0.0) || (op == ">=" && c == 1.0) => {
                format!("the assumption “{}” is on", self.w.opts.get(*i as usize).map_or("", |o| o.label.as_str()))
            }
            VarRef::CdRemains(s) => match op {
                "<" | "<=" => format!("{} is ready within {} sec", self.spell(*s), num(c)),
                _ => format!("{} has more than {} sec of cooldown left", self.spell(*s), num(c)),
            },
            VarRef::Enemies => match (op, c) {
                (">=", n) => format!("{} or more enemies are fighting you", num(n)),
                (">", n) => format!("more than {} {} fighting you", num(n), if n == 1.0 { "enemy is" } else { "enemies are" }),
                ("<", n) if n <= 2.0 => "only one enemy is fighting you".into(),
                _ => format!("{} {} enemies are fighting you", rel("more", "fewer"), num(c)),
            },
            VarRef::EnemiesWithin(yd) => match (op, c) {
                (">=", n) => format!("{} or more enemies are within {} yards", num(n), yd),
                _ => format!("{} {} enemies are within {} yards", rel("more", "fewer"), num(c), yd),
            },
            VarRef::EnemiesActive => match (op, c) {
                (">=", n) => format!("{} or more enemies are fighting you, not counting any you've crowd-controlled", num(n)),
                _ => format!("{} {} enemies are fighting you, not counting any you've crowd-controlled", rel("more", "fewer"), num(c)),
            },
            VarRef::CcNear => match (op, c) {
                ("==", n) | ("<=", n) if n == 0.0 => "no crowd-controlled enemy is close by".into(),
                ("<", n) if n <= 1.0 => "no crowd-controlled enemy is close by".into(),
                _ => format!("{} {} crowd-controlled enemies are close by", rel("more", "fewer"), num(c)),
            },
            VarRef::Talent(_, t) => match (op, c) {
                (">", z) | (">=", z) if z <= 1.0 && (op == ">" || z == 1.0) => format!("you have {}", self.talent(*t)),
                ("==", z) | ("<=", z) if z == 0.0 => format!("you don't have {}", self.talent(*t)),
                _ => format!("you have {} {} points in {}", rel("more", "fewer"), num(c), self.talent(*t)),
            },
            VarRef::Dist(who) => {
                let whom = if *who == Who::Me { "the target" } else { Self::subject(*who) };
                match op {
                    "<" | "<=" => format!("{whom} is within {} yards", num(c)),
                    _ => format!("{whom} is more than {} yards away", num(c)),
                }
            }
            VarRef::Level(Who::Me) => match op {
                ">=" => format!("you're level {} or higher", num(c)),
                "<=" => format!("you're level {} or lower", num(c)),
                ">" => format!("you're above level {}", num(c)),
                "<" => format!("you're below level {}", num(c)),
                _ => format!("you're level {}", num(c)),
            },
            // "3 or more in your group are below 75% health"
            VarRef::Hurt { group, pct: p } => {
                let who = if *group { "in your group" } else { "allies" };
                let below = format!("below {p}% health");
                match (op, c) {
                    (">=", n) => format!("{} or more {who} are {below}", num(n)),
                    (">", n) => format!("more than {} {who} are {below}", num(n)),
                    _ => format!("{} {} {who} are {below}", rel("more", "fewer"), num(c)),
                }
            }
            VarRef::AllyCount(group) => format!("{} {} {} alive", rel("more", "fewer"), num(c), if *group { "in your group are" } else { "allies are" }),
            VarRef::Deficit(who) => format!("{} missing health is {} {}", Self::whose(*who), rel("above", "below"), num(c)),
            VarRef::Stat(who, s) => format!("{} {} is {} {}", Self::whose(*who), s.name().replace('_', " "), rel("above", "below"), num(c)),
            _ => return None,
        })
    }
}

enum Part {
    Skip,
    Text(String),
}

fn mech(m: Mechanic) -> &'static str {
    match m {
        Mechanic::Stun => "stunned", Mechanic::Fear => "feared", Mechanic::Incap => "incapacitated", Mechanic::Root => "rooted",
        Mechanic::Slow => "slowed", Mechanic::Silence => "silenced", Mechanic::Disarm => "disarmed", Mechanic::Freeze => "frozen",
        Mechanic::Immune => "immune",
    }
}

/// The terms of a chain of `and`s, flattened.
fn and_terms<'n>(n: &'n Node, out: &mut Vec<&'n Node>) {
    match n {
        Node::And(a, b) => {
            and_terms(a, out);
            and_terms(b, out);
        }
        _ => out.push(n),
    }
}

fn or_terms<'n>(n: &'n Node, out: &mut Vec<&'n Node>) {
    if let Node::Or(a, b) = n {
        or_terms(a, out);
        or_terms(b, out);
    } else {
        out.push(n);
    }
}

fn article(name: &str) -> String {
    let an = matches!(name.chars().next().map(|c| c.to_ascii_lowercase()), Some('a' | 'e' | 'i' | 'o' | 'u'));
    format!("{} {name}", if an { "an" } else { "a" })
}

fn weapon(wt: u16) -> String {
    let t = WeaponType::ALL[wt as usize];
    let name = match t {
        WeaponType::Sword2h => "two-handed sword", WeaponType::Axe2h => "two-handed axe", WeaponType::Mace2h => "two-handed mace",
        WeaponType::Fist => "fist weapon", WeaponType::Thrown => "thrown weapon", _ => t.name(),
    };
    let an = matches!(name.chars().next(), Some('a' | 'e' | 'i' | 'o' | 'u'));
    format!("{} {name}", if an { "an" } else { "a" })
}

fn hand(h: Hand) -> &'static str {
    match h { Hand::Mh => "main hand", Hand::Oh => "off hand", Hand::Ranged => "ranged slot" }
}

fn creature(c: CreatureType) -> &'static str {
    match c {
        CreatureType::Humanoid => "a humanoid", CreatureType::Beast => "a beast", CreatureType::Undead => "undead", CreatureType::Demon => "a demon",
        CreatureType::Elemental => "an elemental", CreatureType::Dragonkin => "a dragonkin", CreatureType::Giant => "a giant", CreatureType::Mechanical => "a mechanical",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{load_world, LoadOptions};

    #[test]
    fn common_conditions_read_as_plain_english() {
        let w = load_world(&LoadOptions { data_dir: "data".into(), ..Default::default() }).unwrap();
        let c = w.class_index("warrior").unwrap();
        let rot = &w.classes[c].rotations["arms_2h"];
        let said: Vec<String> = rot.actions.iter().filter_map(|a| a.cond.as_ref()).filter_map(|e| condition(&w, Some(c), e)).collect();
        assert!(said.iter().any(|s| s.contains("the target's health is below 20%")), "{said:?}");
        // nothing comes out half translated: a phrase never mixes in formula syntax
        for s in &said {
            assert!(!(s.contains("&&") && s.contains(" is ")), "{s}");
        }
    }
}

/// "fire totem", or "trap" for a hunter's.
fn totem_word(e: u8) -> String {
    match crate::model::ELEMENTS.get(e as usize).copied() {
        Some("trap") | None => "trap".into(),
        Some(x) => format!("{x} totem"),
    }
}
