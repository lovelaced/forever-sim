//! Totems and traps: small units on the ground. A totem goes where its
//! shaman stands, one of each element at a time; it pulses from where it is
//! (Searing Totem at your target within its reach, Magma Totem around
//! itself, Healing Stream and Mana Spring while you're close) or hands its
//! buff to the party within its radius for as long as it stands. A trap arms
//! a moment after it's laid and goes off on the first enemy to come close.

use super::*;

/// A totem's or trap's own bookkeeping.
#[derive(Clone, Copy, Debug)]
pub struct TotemState<'w> {
    pub owner: usize,
    /// Its slot with the owner: an element (`ELEMENTS`), or the trap slot.
    pub slot: u8,
    pub aura: Option<AuraId>,
    pub rank: Option<(SpellId, usize)>,
    pub def: Option<TotemDef>,
    pub until: f64,
    /// A trap: what it does, when it's armed, and how close sets it off.
    pub trap: Option<(&'w [Effect], f64, f64)>,
    /// When a buff totem next checks who's in reach.
    pub next_buff: f64,
    /// Its name, for the fight log.
    pub tag: &'w str,
    /// Kept from the last pull: it goes where the next fight starts.
    pub carried: bool,
    /// How far its pulses and blasts reach.
    pub area: f64,
}

/// The owner's slot for a trap (after the four elements).
pub const TRAP_SLOT: usize = 4;

impl<'w> Sim<'w> {
    fn new_ground_unit(&mut self, owner: usize, name: String, slot: usize, health: f64) -> usize {
        let (team, level, oslot) = (self.actors[owner].team, self.actors[owner].level, self.actors[owner].slot);
        let id = self.new_actor(name, Kind::Totem, team, level, oslot * 32 + 20 + slot as u64);
        let in_combat = self.actors[owner].in_combat;
        let x = &mut self.actors[id];
        x.owner = Some(owner);
        x.fixed = Some([health, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        x.has_res = [true, false, false, false];
        x.in_combat = in_combat;
        let at = self.pos(owner);
        self.place(id, at);
        self.refresh_stats(id);
        self.fill(id);
        id
    }

    /// A shaman puts a totem down (from the totem's aura, cast as a spell).
    pub fn spawn_totem(&mut self, owner: usize, aura: AuraId, rank: Option<(SpellId, usize)>, cx: Cx<'w>) {
        let w = self.w;
        let def = &w.auras[aura as usize];
        let Some(td) = def.totem else { return };
        let slot = td.element as usize;
        // one of each element: a new one replaces the old
        if let Some(old) = self.actors[owner].totems[slot] {
            self.despawn_totem(old);
        }
        let mut c2 = cx;
        c2.me = owner;
        c2.rank = rank;
        let health = def.totem_health.as_ref().map_or(td.health, |h| self.ev(h, &c2).max(1.0));
        let id = self.new_ground_unit(owner, def.name.clone(), slot, health);
        // what a trap leaves behind goes where the trap was
        if let Some(at) = cx.at {
            let x = self.pos(at);
            self.place(id, x);
        }
        let d = self.ev(&def.duration, &c2);
        let until = if d > 0.0 { self.t + d } else { INF };
        self.actors[id].totem = Some(TotemState { owner, slot: slot as u8, aura: Some(aura), rank, def: Some(td), until, trap: None, next_buff: self.t, tag: def.name.as_str(), carried: false, area: td.radius });
        self.actors[owner].totems[slot] = Some(id);
        self.tracef(|s| s.tev("totem", owner, None, def.name.as_str()));
        if td.buff {
            self.totem_buffs(id);
        } else {
            // the totem holds its aura and pulses as its shaman
            let mut c3 = cx;
            c3.at = Some(id);
            self.apply_aura(id, aura, owner, 1, Some(if d > 0.0 { d } else { 0.0 }), rank, c3);
        }
    }

    /// A hunter lays a trap at its feet.
    pub fn spawn_trap(&mut self, owner: usize, then: &'w [Effect], (arm, radius, lasts, area): (f64, f64, f64, f64), rank: Option<(SpellId, usize)>, name: &'w str) {
        if let Some(old) = self.actors[owner].totems[TRAP_SLOT] {
            self.despawn_totem(old);
        }
        let id = self.new_ground_unit(owner, name.to_string(), TRAP_SLOT, 1.0);
        let t = self.t;
        self.actors[id].totem = Some(TotemState { owner, slot: TRAP_SLOT as u8, aura: None, rank, def: None, until: t + lasts, trap: Some((then, t + arm, radius)), next_buff: INF, tag: name, carried: false, area });
        self.actors[owner].totems[TRAP_SLOT] = Some(id);
    }

    /// Hand a buff totem's aura to the party within its radius, and take it
    /// off anyone who has walked out.
    pub fn totem_buffs(&mut self, t: usize) {
        let Some(st) = self.actors[t].totem else { return };
        let (Some(td), Some(aura)) = (st.def, st.aura) else { return };
        if !td.buff || !self.actors[t].alive {
            return;
        }
        let owner = st.owner;
        let members: Vec<usize> = self.allies(owner, false).collect();
        for m in members {
            let inside = self.dist(t, m) <= td.radius + 1e-6;
            let has = self.actors[m].auras.iter().position(|i| i.def == aura && i.src == owner);
            if inside && has.is_none() {
                let mut cx = Cx::new(owner, Some(m), self.w.auras[aura as usize].name.as_str());
                cx.rank = st.rank;
                self.totem_buffing = true;
                let left = (st.until - self.t).max(0.0);
                self.apply_aura(m, aura, owner, 1, Some(if st.until == INF { 0.0 } else { left }), st.rank, cx);
                self.totem_buffing = false;
            } else if !inside {
                if let Some(k) = has {
                    self.remove_aura_at(m, k, false);
                }
            }
        }
        if let Some(s) = self.actors[t].totem.as_mut() {
            s.next_buff = self.t + 2.0;
        }
    }

    /// A totem or trap goes away (its time is up, it was destroyed, or it
    /// wasn't carried to the next pull); its buffs go with it.
    pub fn despawn_totem(&mut self, t: usize) {
        let Some(st) = self.actors[t].totem else { return };
        let x = &mut self.actors[t];
        x.alive = false;
        x.totem = None;
        self.stop(t);
        if let (Some(aura), Some(td)) = (st.aura, st.def) {
            if td.buff {
                let members: Vec<usize> = (0..self.actors.len()).filter(|&i| self.actors[i].team == self.actors[st.owner].team).collect();
                for m in members {
                    if let Some(k) = self.actors[m].auras.iter().position(|i| i.def == aura && i.src == st.owner) {
                        self.remove_aura_at(m, k, false);
                    }
                }
            }
        }
        if self.actors[st.owner].totems[st.slot as usize] == Some(t) {
            self.actors[st.owner].totems[st.slot as usize] = None;
        }
    }

    /// A totem's and trap's own business at this moment: running out,
    /// going off, checking who's in reach of its buff. Returns false when
    /// it's gone.
    pub fn tend_totem(&mut self, a: usize) -> bool {
        let Some(st) = self.actors[a].totem else { return true };
        let t = self.t;
        if t >= st.until - EPS {
            self.despawn_totem(a);
            return false;
        }
        if let Some((then, arm_at, radius)) = st.trap {
            if t >= arm_at - EPS {
                let team = self.actors[a].team;
                let hit = self.live.iter().copied().filter(|&i| {
                    let y = &self.actors[i];
                    y.alive && !y.evaded && y.team != team && !y.is_pet()
                }).filter(|&i| self.dist(a, i) <= radius + 1e-6).min_by(|&x, &y| self.dist(a, x).partial_cmp(&self.dist(a, y)).unwrap().then(x.cmp(&y)));
                if let Some(e) = hit {
                    let mut cx = Cx::new(st.owner, Some(e), st.tag);
                    cx.at = Some(a);
                    cx.rank = st.rank;
                    cx.sm = self.spell_mod_vals(st.owner, st.rank.map(|r| r.0), None);
                    self.tracef(|s| s.tev("trap", st.owner, Some(e), st.tag));
                    // it goes off, then it's gone
                    if let Some(s) = self.actors[a].totem.as_mut() {
                        s.trap = None;
                    }
                    self.run_effects(then, cx);
                    self.despawn_totem(a);
                    return false;
                }
            }
        }
        if st.def.map_or(false, |d| d.buff) && t >= st.next_buff - EPS {
            self.totem_buffs(a);
        }
        true
    }

    /// The next moment a totem or trap needs a look.
    pub fn totem_next(&self, a: usize) -> f64 {
        let Some(st) = self.actors[a].totem else { return INF };
        let mut n = st.until;
        if let Some((_, arm_at, _)) = st.trap {
            if arm_at > self.t {
                n = n.min(arm_at);
            }
        }
        if st.def.map_or(false, |d| d.buff) && st.next_buff > self.t {
            n = n.min(st.next_buff);
        }
        n
    }

    /// Whether a totem's pulse reaches anything right now (Searing Totem
    /// needs your target near it, the water totems need you near).
    pub fn totem_reaches(&self, a: usize) -> bool {
        let Some(st) = self.actors[a].totem else { return true };
        let Some(td) = st.def else { return true };
        match td.reach {
            TotemReach::Area => true,
            TotemReach::Owner => self.dist(a, st.owner) <= td.radius + 1e-6,
            TotemReach::Target => self.actors[st.owner].target.map_or(false, |t| self.actors[t].alive && self.dist(a, t) <= td.radius + 1e-6),
        }
    }

    /// Your totem of an element, if it's up.
    pub fn totem_of(&self, a: usize, element: u8) -> Option<usize> {
        let o = self.actors[a].owner.filter(|_| self.actors[a].kind == Kind::Totem).unwrap_or(a);
        self.actors[o].totems.get(element as usize).copied().flatten().filter(|&t| self.actors[t].alive)
    }

    /// Whether a spell puts a totem on the ground.
    pub fn places_totem(&self, spell: SpellId) -> bool {
        let w = self.w;
        w.spells[spell as usize].effects.iter().any(|e| matches!(e, Effect::ApplyAura { aura, .. } if w.auras[*aura as usize].totem.is_some()))
    }

    /// Leaving the spot: the totems stay behind unless the next fight comes
    /// to them (`carry`); a trap never comes along.
    pub fn leave_totems(&mut self, a: usize, carry: bool) {
        for k in 0..self.actors[a].totems.len() {
            let Some(t) = self.actors[a].totems[k] else { continue };
            if carry && k != TRAP_SLOT {
                if let Some(s) = self.actors[t].totem.as_mut() {
                    s.carried = true;
                }
            } else {
                self.despawn_totem(t);
            }
        }
    }

    /// The next fight has found you near the totems you kept: they stand
    /// where you are now.
    pub fn bring_totems(&mut self, a: usize) {
        for k in 0..self.actors[a].totems.len() {
            let Some(t) = self.actors[a].totems[k] else { continue };
            if self.actors[t].totem.map_or(false, |s| s.carried) {
                let at = self.pos(a);
                self.place(t, at);
                if let Some(s) = self.actors[t].totem.as_mut() {
                    s.carried = false;
                    s.next_buff = s.next_buff.min(self.t);
                }
            }
        }
    }
}
