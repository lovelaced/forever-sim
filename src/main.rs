use clap::{Parser, Subcommand};
use forever_sim::build::{load_build, Build};
use forever_sim::mc::{self, McCfg, Variant};
use forever_sim::model::*;
use forever_sim::scenario::{run_rep, RepInput};
use forever_sim::validate::modelled_talents;
use forever_sim::world::{load_world, LoadOptions};
use std::path::PathBuf;
use std::process::exit;

#[derive(Parser)]
#[command(name = "fsim", version, about = "Monte Carlo combat simulator for WoW: Forever (data-driven)")]
struct Cli {
    /// Data directory (contains base/ and patches/)
    #[arg(long, global = true, default_value = "data")]
    data: PathBuf,
    /// Only apply patches up to this client build (e.g. 1.60.1.70338)
    #[arg(long, global = true)]
    at_build: Option<String>,
    /// Extra overlay file(s) applied last, for "what if" changes
    #[arg(long = "patch", global = true)]
    patches: Vec<PathBuf>,
    /// Ignore data/patches entirely
    #[arg(long, global = true)]
    no_patches: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Args, Clone)]
struct McArgs {
    /// Character level override (talents are trimmed using the build's `order`)
    #[arg(long)]
    level: Option<u32>,
    /// Opponent build for duel scenarios
    #[arg(long)]
    vs: Option<PathBuf>,
    #[arg(long, default_value_t = 20261104)]
    seed: u64,
    /// Minimum replications
    #[arg(long, default_value_t = 64)]
    min_reps: usize,
    /// Maximum replications
    #[arg(long, default_value_t = 4000)]
    max_reps: usize,
    /// Replications per batch between convergence checks
    #[arg(long, default_value_t = 64)]
    batch: usize,
    /// Target CI half-width (relative for means, absolute for win rates)
    #[arg(long, default_value_t = 0.01)]
    precision: f64,
    /// Confidence level for intervals
    #[arg(long, default_value_t = 0.95)]
    conf: f64,
    /// Worker threads (0 = all cores)
    #[arg(long, default_value_t = 0)]
    threads: usize,
    /// Override an assumption: --opt name=value (repeatable)
    #[arg(long = "opt")]
    opts: Vec<String>,
    /// Write full results as JSON
    #[arg(long)]
    json: Option<PathBuf>,
    /// Show every metric, including per-ability breakdowns
    #[arg(long)]
    all: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Validate the data and optional build files; show which talents are modelled
    Check { builds: Vec<PathBuf> },
    /// Check the class data against WoW: Forever's spell tables (data/reference/spellbook.json)
    Audit {
        /// Only this class
        class: Option<String>,
        #[arg(long, default_value = "data/reference/spellbook.json")]
        spellbook: PathBuf,
        /// Deliberate differences, each with a reason (a file, or a folder of <class>.toml files)
        #[arg(long, default_value = "data/reference/accepted")]
        accepted: PathBuf,
        /// Also list what matches
        #[arg(long)]
        all: bool,
        /// Exit with an error if anything differs without a reason
        #[arg(long)]
        strict: bool,
        /// Write every row as JSON
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// List classes, talents <class>, spells <class>, rotations, scenarios, gear, enemies, options, races
    List { what: String, class: Option<String> },
    /// Run a scenario for one or more builds (paired comparison when several)
    Run {
        scenario: String,
        builds: Vec<PathBuf>,
        #[command(flatten)]
        mc: McArgs,
    },
    /// Run a scenario at several levels using the build's talent order
    Scan {
        scenario: String,
        build: PathBuf,
        /// Comma-separated levels
        #[arg(long, default_value = "20,30,40,50,60")]
        levels: String,
        #[command(flatten)]
        mc: McArgs,
    },
    /// Compare builds on the current data against the same builds with patch file(s) applied
    Whatif {
        scenario: String,
        builds: Vec<PathBuf>,
        /// Overlay(s) to test (a new client build's patch file, or a what-if change)
        #[arg(long = "with", required = true)]
        with: Vec<PathBuf>,
        #[command(flatten)]
        mc: McArgs,
    },
    /// Serve the web front end and its API locally (http://localhost:PORT)
    Serve {
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Folder with index.html (the web front end)
        #[arg(long, default_value = "web")]
        web: PathBuf,
    },
    /// Print the merged data (after patches) at a dotted path, e.g. classes.warrior.spells.bloodthirst
    Show { path: String },
    /// Answer one web API request (JSON, from the argument or stdin), e.g. fsim api '{"op":"catalog"}'
    Api { request: Option<String> },
    /// Show a build's character sheet (unbuffed, and after its precombat buffs)
    Sheet {
        build: PathBuf,
        #[arg(long)]
        level: Option<u32>,
    },
    /// Print a combat log of a single replication
    Log {
        scenario: String,
        build: PathBuf,
        #[arg(long)]
        vs: Option<PathBuf>,
        #[arg(long)]
        level: Option<u32>,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value_t = 200)]
        lines: usize,
    },
}

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {msg}");
    exit(1)
}

fn parse_opts(w: &World, b: &Build, sc: &ScenarioDef, extra: &[String]) -> Vec<f64> {
    let mut v: Vec<f64> = w.opts.iter().map(|o| o.default).collect();
    for &(i, x) in &sc.opts {
        v[i as usize] = x;
    }
    for &(i, x) in &b.opts {
        v[i as usize] = x;
    }
    for kv in extra {
        let (k, x) = kv.split_once('=').unwrap_or_else(|| die(format!("--opt expects name=value, got '{kv}'")));
        let i = w.opt_index(k).unwrap_or_else(|| die(format!("unknown option '{k}' (fsim list options)")));
        v[i as usize] = x.parse().unwrap_or_else(|_| die(format!("--opt {k}: not a number")));
    }
    v
}

fn describe(w: &World, b: &Build) -> String {
    format!(
        "{} — {} {} {}, talents {}, gear {}, rotation {}",
        b.name,
        w.races[b.race].name,
        w.classes[b.class].name,
        b.level,
        b.tree_split(w),
        b.gear_id,
        b.rotation
    )
}

fn fmt_num(x: f64) -> String {
    let x = if x == 0.0 { 0.0 } else { x }; // no "-0"
    let a = x.abs();
    if a >= 1000.0 {
        format!("{x:.0}")
    } else if a >= 100.0 {
        format!("{x:.1}")
    } else if a >= 1.0 {
        format!("{x:.2}")
    } else {
        format!("{x:.3}")
    }
}

fn print_summary(sums: &[mc::MetricSummary], primary: &str, all: bool) {
    let headline: Vec<&mc::MetricSummary> = sums.iter().filter(|s| !s.name.contains(':')).collect();
    let mut order: Vec<&mc::MetricSummary> = headline.iter().copied().filter(|s| s.name == primary).collect();
    order.extend(headline.iter().copied().filter(|s| s.name != primary));
    for s in order {
        let star = if s.name == primary { "*" } else { " " };
        let centre = if s.kind == "median" { s.median } else { s.mean };
        let kind = match s.kind {
            "proportion" => "rate",
            "median" => "median",
            _ => "",
        };
        println!(
            " {star}{:<26} {:>10}  [{:>9}, {:>9}]  sd {:>8} {}",
            s.name,
            fmt_num(centre),
            fmt_num(s.lo),
            fmt_num(s.hi),
            fmt_num(s.sd),
            kind
        );
    }
    let mut groups: std::collections::BTreeMap<&str, Vec<&mc::MetricSummary>> = Default::default();
    for s in sums.iter().filter(|s| s.name.contains(':')) {
        groups.entry(s.name.split(':').next().unwrap()).or_default().push(s);
    }
    for (g, mut v) in groups {
        v.sort_by(|a, b| b.mean.partial_cmp(&a.mean).unwrap());
        let shown: Vec<&&mc::MetricSummary> = if all { v.iter().collect() } else { v.iter().take(8).collect() };
        let parts: Vec<String> = shown.iter().map(|s| format!("{} {}", s.name.split_once(':').unwrap().1, fmt_num(s.mean))).collect();
        println!("  {g}: {}", parts.join(" · "));
    }
}

fn main() {
    let cli = Cli::parse();
    let lo = LoadOptions { data_dir: cli.data.clone(), at_build: cli.at_build.clone(), extra_patches: cli.patches.clone(), skip_patches: cli.no_patches };
    let w = load_world(&lo).unwrap_or_else(|e| die(e));
    let header = format!(
        "Forever data {}{}",
        if w.meta.data_build.is_empty() { "(unversioned)".to_string() } else { format!("build {}", w.meta.data_build) },
        if w.meta.patches.is_empty() { String::new() } else { format!(" + patches: {}", w.meta.patches.join(", ")) }
    );
    match cli.cmd {
        Cmd::Check { builds } => {
            println!("{header}");
            println!(
                "OK: {} classes, {} races, {} spells, {} auras, {} pets, {} gear kits, {} enemies, {} scenarios, {} options",
                w.classes.len(), w.races.len(), w.spells.len(), w.auras.len(), w.pets.len(), w.gear.len(), w.enemies.len(), w.scenarios.len(), w.opts.len()
            );
            for (ci, c) in w.classes.iter().enumerate() {
                let m = modelled_talents(&w, ci);
                let n = m.iter().filter(|&&x| x).count();
                println!("  {:<8} {:>2}/{:<2} talents modeled, {} rotations: {}", c.id, n, c.talents.len(), c.rotations.len(), c.rotations.keys().cloned().collect::<Vec<_>>().join(", "));
            }
            let mut bad = false;
            for p in builds {
                match load_build(&w, &p, None) {
                    Ok(b) => {
                        println!("\n{}", describe(&w, &b));
                        for wn in &b.warnings {
                            println!("  warning: {wn}");
                        }
                        let m = modelled_talents(&w, b.class);
                        let c = &w.classes[b.class];
                        let un: Vec<String> = c
                            .talents
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| b.talents[*i] > 0 && !m[*i])
                            .map(|(i, t)| format!("{} {}", t.name, b.talents[i]))
                            .collect();
                        if un.is_empty() {
                            println!("  every talent with points is modeled");
                        } else {
                            println!("  points in talents the sim does not model (no effect on results): {}", un.join(", "));
                        }
                    }
                    Err(e) => {
                        bad = true;
                        println!("\n{}: {e}", p.display())
                    }
                }
            }
            if bad {
                exit(1);
            }
        }
        Cmd::Audit { class, spellbook, accepted, all, strict, json } => {
            use forever_sim::audit::{Accepted, Audit, Book};
            let book = std::fs::read_to_string(&spellbook).map_err(|e| format!("{}: {e}", spellbook.display())).and_then(|t| Book::parse(&t)).unwrap_or_else(|e| die(e));
            let mut texts = Vec::new();
            if accepted.is_dir() {
                let mut files: Vec<PathBuf> = std::fs::read_dir(&accepted).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().map_or(false, |x| x == "toml")).collect();
                files.sort();
                for f in files {
                    texts.push(std::fs::read_to_string(&f).unwrap_or_else(|e| die(format!("{}: {e}", f.display()))));
                }
            } else if let Ok(t) = std::fs::read_to_string(&accepted) {
                texts.push(t);
            }
            let acc = Accepted::parse_all(&texts).unwrap_or_else(|e| die(e));
            let mut a = Audit::run(&w, &book, &acc, class.as_deref());
            a.run_talents(class.as_deref());
            print!("{}", a.report(all));
            if let Some(p) = json {
                std::fs::write(&p, serde_json::to_string_pretty(&a.rows).unwrap()).unwrap_or_else(|e| die(format!("{}: {e}", p.display())));
            }
            if strict && a.failures() > 0 {
                eprintln!("{} differences without a reason", a.failures());
                exit(1);
            }
        }
        Cmd::List { what, class } => {
            let cls = |c: &Option<String>| -> usize {
                let id = c.as_deref().unwrap_or_else(|| die("which class?"));
                w.class_index(id).unwrap_or_else(|| die(format!("unknown class '{id}'")))
            };
            match what.as_str() {
                "classes" => {
                    for c in &w.classes {
                        println!("{:<8} {} — trees: {}", c.id, c.name, c.trees.join(" / "));
                    }
                }
                "races" => {
                    for r in &w.races {
                        println!("{:<14} {:<22} {:<9} {}", r.id, r.name, r.faction, r.classes.join(", "));
                    }
                }
                "talents" => {
                    let ci = cls(&class);
                    let m = modelled_talents(&w, ci);
                    let c = &w.classes[ci];
                    for tree in &c.trees {
                        println!("{tree}");
                        let mut ts: Vec<(usize, &TalentDef)> = c.talents.iter().enumerate().filter(|(_, t)| &t.tree == tree).collect();
                        ts.sort_by_key(|(_, t)| t.points);
                        for (i, t) in ts {
                            let req = t.requires.map(|r| format!(" (needs {})", c.talents[r as usize].name)).unwrap_or_default();
                            println!("  {:>2}pt {} {:<32} {}/{}{}  {}", t.points, if m[i] { "✓" } else { "·" }, t.id, 0, t.ranks, req, t.text);
                        }
                    }
                    println!("\n✓ = used by the simulation, · = present for talent legality only");
                }
                "spells" => {
                    let ci = cls(&class);
                    for s in w.spells.iter().filter(|s| s.class == Some(ci)) {
                        let lv = s.ranks.first().map_or(s.learn_level, |r| r.level);
                        let tal = s.talent.map(|t| format!(" [talent {}]", w.classes[ci].talents[t as usize].id)).unwrap_or_default();
                        println!("{:<28} lvl {:>2} ranks {:>2}{}  {}", s.id, lv, s.ranks.len(), tal, s.source);
                    }
                }
                "rotations" => {
                    for c in &w.classes {
                        for (id, r) in &c.rotations {
                            println!("{:<8} {:<20} {}", c.id, id, r.desc);
                        }
                    }
                }
                "scenarios" => {
                    for s in w.scenarios.values() {
                        println!("{:<22} primary {:<16} {}", s.id, s.primary, s.desc);
                    }
                }
                "gear" => {
                    for g in w.gear.values() {
                        println!("{:<18} levels {:?}  {}", g.id, g.anchors.iter().map(|a| a.0).collect::<Vec<_>>(), g.desc);
                    }
                }
                "enemies" => {
                    for e in w.enemies.values() {
                        println!("{:<20} {}", e.id, e.name);
                    }
                }
                "options" => {
                    for o in &w.opts {
                        println!("{:<28} default {:<8} {}", o.name, o.default, o.desc);
                    }
                }
                x => die(format!("unknown list '{x}'")),
            }
        }
        Cmd::Run { scenario, builds, mc: a } => {
            let sc = w.scenarios.get(&scenario).unwrap_or_else(|| die(format!("unknown scenario '{scenario}' (fsim list scenarios)")));
            if builds.is_empty() {
                die("give at least one build file");
            }
            let opp = a.vs.as_ref().map(|p| load_build(&w, p, a.level).unwrap_or_else(|e| die(e)));
            if matches!(sc.kind, ScenarioKind::Duel { .. }) && opp.is_none() {
                die("duel scenarios need an opponent: --vs <build.toml>");
            }
            let mut variants: Vec<Variant> = builds
                .iter()
                .map(|p| {
                    let b = load_build(&w, p, a.level).unwrap_or_else(|e| die(e));
                    if let Some(e) = forever_sim::scenario::unfit(sc, &w, &b) {
                        die(e);
                    }
                    let opts = parse_opts(&w, &b, sc, &a.opts);
                    Variant { world: 0, name: b.name.clone(), build: b, opponent: opp.clone(), opts }
                })
                .collect();
            unique_names(&w, &mut variants);
            run_and_report(&[&w], &header, sc, variants, &a);
        }
        Cmd::Scan { scenario, build, levels, mc: a } => {
            let sc = w.scenarios.get(&scenario).unwrap_or_else(|| die(format!("unknown scenario '{scenario}'")));
            println!("{header}\nscenario {} — {}\n", sc.id, sc.desc);
            println!("{:>5}  {:<12} {:>10}  {:>21}  reps", "level", "talents", sc.primary, "95% interval");
            for l in levels.split(',') {
                let lvl: u32 = l.trim().parse().unwrap_or_else(|_| die("levels must be numbers"));
                let b = match load_build(&w, &build, Some(lvl)) {
                    Ok(b) => b,
                    Err(e) => {
                        println!("{lvl:>5}  skipped: {e}");
                        continue;
                    }
                };
                let opp = a.vs.as_ref().map(|p| load_build(&w, p, Some(lvl)).unwrap_or_else(|e| die(e)));
                let opts = parse_opts(&w, &b, sc, &a.opts);
                let split = b.tree_split(&w);
                let v = vec![Variant { world: 0, name: b.name.clone(), build: b, opponent: opp, opts }];
                let cfg = McCfg { seed: a.seed, min_reps: a.min_reps, max_reps: a.max_reps, batch: a.batch, precision: a.precision, conf: a.conf, threads: a.threads };
                let res = mc::run(&[&w], &sc.id, &v, &cfg);
                let sums = mc::summarise(&res.values[0], a.conf);
                if let Some(s) = sums.iter().find(|s| s.name == sc.primary) {
                    let c = if s.kind == "median" { s.median } else { s.mean };
                    println!("{lvl:>5}  {split:<12} {:>10}  [{:>9}, {:>9}]  {}", fmt_num(c), fmt_num(s.lo), fmt_num(s.hi), res.reps);
                }
            }
        }
        Cmd::Whatif { scenario, builds, with, mc: a } => {
            let mut lo2 = lo.clone();
            lo2.extra_patches.extend(with.iter().cloned());
            let w2 = load_world(&lo2).unwrap_or_else(|e| die(format!("with {}: {e}", with.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "))));
            let sc = w.scenarios.get(&scenario).unwrap_or_else(|| die(format!("unknown scenario '{scenario}'")));
            if builds.is_empty() {
                die("give at least one build file");
            }
            let tag = with.iter().map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()).collect::<Vec<_>>().join("+");
            for p in &builds {
                let mut variants = Vec::new();
                for (wi, ww) in [&w, &w2].iter().enumerate() {
                    let sc_w = ww.scenarios.get(&scenario).unwrap_or_else(|| die(format!("scenario '{scenario}' missing after the patch")));
                    let b = load_build(ww, p, a.level).unwrap_or_else(|e| die(e));
                    let opp = a.vs.as_ref().map(|q| load_build(ww, q, a.level).unwrap_or_else(|e| die(e)));
                    let opts = parse_opts(ww, &b, sc_w, &a.opts);
                    let name = if wi == 0 { format!("{} (current)", b.name) } else { format!("{} (+{tag})", b.name) };
                    variants.push(Variant { world: wi, name, build: b, opponent: opp, opts });
                }
                run_and_report(&[&w, &w2], &header, sc, variants, &a);
                println!();
            }
        }
        Cmd::Serve { port, web } => serve(port, web),
        Cmd::Api { request } => {
            let req = request.unwrap_or_else(|| {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).unwrap_or_else(|e| die(e));
                s
            });
            println!("{}", forever_sim::api::handle(&req));
        }
        Cmd::Show { path } => {
            let (tree, _) = forever_sim::world::load_tree(&lo).unwrap_or_else(|e| die(e));
            let mut cur = &tree;
            for part in path.split('.').filter(|p| !p.is_empty()) {
                cur = cur.get(part).unwrap_or_else(|| die(format!("nothing at '{part}' in {path}")));
            }
            match cur {
                toml::Value::Table(t) => println!("{}", toml::to_string_pretty(t).unwrap_or_default()),
                v => println!("{v}"),
            }
        }
        Cmd::Sheet { build, level } => {
            let b = load_build(&w, &build, level).unwrap_or_else(|e| die(e));
            println!("{}", describe(&w, &b));
            for wn in &b.warnings {
                println!("  note: {wn}");
            }
            let opts: Vec<f64> = {
                let mut v: Vec<f64> = w.opts.iter().map(|o| o.default).collect();
                for &(i, x) in &b.opts {
                    v[i as usize] = x;
                }
                v
            };
            let mut sim = forever_sim::engine::Sim::new(&w, 1, opts);
            let p = sim.spawn_player(&b, 0, 0);
            let before = sim.sheet(p);
            sim.precombat(p);
            let after = sim.sheet(p);
            println!("\n{:<22} {:>10} {:>12}", "", "unbuffed", "precombat");
            for ((k, v0), (_, v1)) in before.iter().zip(after.iter()) {
                if *v0 == 0.0 && *v1 == 0.0 {
                    continue;
                }
                println!("{k:<22} {:>10} {:>12}", fmt_num(*v0), fmt_num(*v1));
            }
            let auras: Vec<String> = sim.actors[p].auras.iter().map(|a| w.auras[a.def as usize].name.clone()).collect();
            if !auras.is_empty() {
                println!("\nprecombat auras: {}", auras.join(", "));
            }
        }
        Cmd::Log { scenario, build, vs, level, seed, lines } => {
            let sc = w.scenarios.get(&scenario).unwrap_or_else(|| die(format!("unknown scenario '{scenario}'")));
            let b = load_build(&w, &build, level).unwrap_or_else(|e| die(e));
            if let Some(e) = forever_sim::scenario::unfit(sc, &w, &b) {
                die(e);
            }
            let opp = vs.map(|p| load_build(&w, &p, level).unwrap_or_else(|e| die(e)));
            let opts = parse_opts(&w, &b, sc, &[]);
            let (vals, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: opp.as_ref(), opts: &opts, seed, log: true });
            for l in log.unwrap_or_default().iter().take(lines) {
                println!("{l}");
            }
            println!("--");
            for (k, v) in vals.iter().filter(|(k, _)| !k.contains(':')) {
                println!("{k:<26} {}", fmt_num(*v));
            }
        }
    }
}

fn run_and_report(worlds: &[&World], header: &str, sc: &ScenarioDef, variants: Vec<Variant>, a: &McArgs) {
    let cfg = McCfg { seed: a.seed, min_reps: a.min_reps, max_reps: a.max_reps, batch: a.batch, precision: a.precision, conf: a.conf, threads: a.threads };
    let t0 = std::time::Instant::now();
    let res = mc::run(worlds, &sc.id, &variants, &cfg);
    let secs = t0.elapsed().as_secs_f64();
    println!("{header}");
    println!("scenario {} — {}", sc.id, sc.desc);
    let prec: Vec<String> = res.values.iter().map(|v| format!("{:.2}%", 100.0 * mc::precision_of(v, &sc.primary, a.conf))).collect();
    println!(
        "{} replications per build ({}; {} CI half-width on {}: {}), seed {}, {:.1}s\n",
        res.reps,
        if res.converged { "converged" } else { "hit the replication cap" },
        format!("{:.0}%", a.conf * 100.0),
        sc.primary,
        prec.join(" / "),
        a.seed,
        secs
    );
    let mut json = serde_json::Map::new();
    for (vi, v) in variants.iter().enumerate() {
        let w = worlds[v.world];
        // the healing tests play the build's healing style
        let mut shown = v.build.clone();
        if let (ScenarioKind::Healing(_), Some(r)) = (&sc.kind, &v.build.heal_rotation) {
            shown.rotation = r.clone();
        }
        println!("== {}", describe(w, &shown));
        if let Some(o) = &v.opponent {
            println!("   vs {}", describe(w, o));
        }
        for wn in &v.build.warnings {
            println!("   warning: {wn}");
        }
        let sums = mc::summarise(&res.values[vi], a.conf);
        print_summary(&sums, &sc.primary, a.all);
        println!();
        json.insert(v.name.clone(), serde_json::to_value(&sums).unwrap());
    }
    if variants.len() > 1 {
        let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
        let mut metrics: Vec<String> = vec![sc.primary.clone()];
        let first = mc::summarise(&res.values[0], a.conf);
        for s in first.iter().filter(|s| !s.name.contains(':') && s.name != sc.primary) {
            metrics.push(s.name.clone());
        }
        let cmp = mc::compare(&names, &res, &metrics, a.conf);
        println!("Paired differences vs {} (same seeds per replication):", names[0]);
        for c in &cmp {
            if c.metric != sc.primary && !a.all {
                continue;
            }
            println!(
                "  {:<22} {:<34} {:>+10}  [{:>+9}, {:>+9}]  {}  CRN variance reduction x{:.1}",
                c.metric,
                c.variant,
                fmt_num(c.diff),
                fmt_num(c.lo),
                fmt_num(c.hi),
                if c.significant { "significant" } else { "not significant" },
                c.variance_reduction
            );
        }
        json.insert("_paired".into(), serde_json::to_value(&cmp).unwrap());
    }
    if let Some(p) = &a.json {
        let mut root = serde_json::Map::new();
        root.insert("scenario".into(), sc.id.clone().into());
        root.insert("data".into(), header.into());
        root.insert("reps".into(), res.reps.into());
        root.insert("converged".into(), res.converged.into());
        root.insert("results".into(), serde_json::Value::Object(json));
        std::fs::write(p, serde_json::to_string_pretty(&root).unwrap()).unwrap_or_else(|e| die(e));
        println!("\nwrote {}", p.display());
    }
}

/// Builds of different classes can share a name ("Holy", "Protection"):
/// those get their class added, so every result has its own name.
fn unique_names(w: &World, variants: &mut [Variant]) {
    let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
    for v in variants.iter_mut() {
        if names.iter().filter(|n| **n == v.name).count() > 1 {
            v.name = format!("{} ({})", v.name, w.classes[v.build.class].id);
        }
    }
}

/// A tiny local web server for the front end: static files from `web`, and
/// POST /api answered by the same JSON interface the WebAssembly build uses.
fn serve(port: u16, web: PathBuf) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|e| die(format!("port {port}: {e}")));
    println!("fsim web front end on http://localhost:{port}  (Ctrl-C to stop)");
    let web = std::sync::Arc::new(web);
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let web = web.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(match stream.try_clone() {
                Ok(s) => s,
                Err(_) => return,
            });
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                return;
            }
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let target = parts.next().unwrap_or("/").to_string();
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    if k.trim().eq_ignore_ascii_case("content-length") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
            }
            let path = target.split('?').next().unwrap_or("/").to_string();
            let (status, ctype, body): (&str, &str, Vec<u8>) = if method == "POST" && path == "/api" {
                let mut buf = vec![0u8; len];
                if reader.read_exact(&mut buf).is_err() {
                    return;
                }
                let out = forever_sim::api::handle(&String::from_utf8_lossy(&buf));
                ("200 OK", "application/json", out.into_bytes())
            } else if method == "GET" {
                let rel = if path == "/" { "index.html".to_string() } else { path.trim_start_matches('/').to_string() };
                if rel.split('/').any(|seg| seg == ".." || seg.is_empty()) {
                    ("400 Bad Request", "text/plain", b"bad path".to_vec())
                } else {
                    match std::fs::read(web.join(&rel)) {
                        Ok(b) => {
                            let ext = rel.rsplit('.').next().unwrap_or("");
                            let ct = match ext {
                                "html" => "text/html; charset=utf-8",
                                "css" => "text/css",
                                "js" | "mjs" => "application/javascript",
                                "json" => "application/json",
                                "wasm" => "application/wasm",
                                "svg" => "image/svg+xml",
                                "png" => "image/png",
                                "jpg" | "jpeg" => "image/jpeg",
                                "webp" => "image/webp",
                                "woff2" => "font/woff2",
                                "ico" => "image/x-icon",
                                _ => "application/octet-stream",
                            };
                            ("200 OK", ct, b)
                        }
                        Err(_) => ("404 Not Found", "text/plain", b"not found".to_vec()),
                    }
                }
            } else {
                ("405 Method Not Allowed", "text/plain", b"method not allowed".to_vec())
            };
            let head = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        });
    }
}
