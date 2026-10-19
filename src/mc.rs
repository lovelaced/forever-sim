//! Monte Carlo driver.
//!
//! * Replication r of every variant uses the same seed (common random
//!   numbers), so builds are compared on identical dice.
//! * Replications run in parallel but each result is stored by index and
//!   summarised in index order, so output never depends on thread count.
//! * Replications run in batches until the primary metric's confidence
//!   interval is narrower than the requested precision (or a cap is hit).

use crate::build::Build;
use crate::model::*;
use crate::rng::{derive_seed, hash_str};
use crate::scenario::{run_rep, RepInput, Values};
use crate::stats::*;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct McCfg {
    pub seed: u64,
    pub min_reps: usize,
    pub max_reps: usize,
    pub batch: usize,
    /// Relative CI half-width target for means; absolute for proportions.
    pub precision: f64,
    pub conf: f64,
    pub threads: usize,
}

impl Default for McCfg {
    fn default() -> Self {
        McCfg { seed: 20261104, min_reps: 64, max_reps: 4000, batch: 64, precision: 0.01, conf: 0.95, threads: 0 }
    }
}

pub struct Variant {
    /// Index into the worlds passed to `run` (0 unless comparing data versions).
    pub world: usize,
    pub name: String,
    pub build: Build,
    pub opponent: Option<Build>,
    pub opts: Vec<f64>,
}

pub struct McResult {
    pub reps: usize,
    pub converged: bool,
    /// [variant][replication] -> metrics
    pub values: Vec<Vec<Values>>,
}

/// Metrics reported as proportions (Wilson interval) rather than means.
pub fn is_proportion(k: &str) -> bool {
    matches!(k, "win" | "loss" | "draw" | "timeout" | "first" | "died" | "win_at_reference")
}
/// Duel scores (a win 1, a draw or timeout half): converged on an absolute
/// half-width, like a win rate.
pub fn is_score(k: &str) -> bool {
    matches!(k, "score" | "score_first" | "score_second")
}
/// Metrics summarised by their median (distribution-free interval).
pub fn is_median(k: &str) -> bool {
    matches!(k, "breakeven" | "breakeven_scale" | "heal_breakeven" | "heal_breakeven_scale")
}

/// How a metric is summarised and compared.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetricKind {
    /// A mean, with a Student-t range.
    Mean,
    /// A mean between 0 and 1 (a score or a rate), each replication
    /// averaging `per_rep` fights: a Wilson range that stays inside 0..1.
    Bounded { per_rep: f64 },
    /// A median of values found on a grid whose cells are `rel_cell` of the
    /// value wide (a breakeven's bisection): a distribution-free range, and
    /// a paired bootstrap for differences, never narrower than one cell.
    Median { rel_cell: f64 },
}

/// A metric's kind without its scenario (one fight a replication, no grid);
/// `scenario::metric_kind` knows better.
pub fn default_kind(k: &str) -> MetricKind {
    if is_proportion(k) || is_score(k) || k == "holds_standard" || k.starts_with("score:") {
        MetricKind::Bounded { per_rep: 1.0 }
    } else if is_median(k) {
        MetricKind::Median { rel_cell: 0.0 }
    } else {
        MetricKind::Mean
    }
}

pub fn rep_seed(cfg: &McCfg, sc: &ScenarioDef, r: usize) -> u64 {
    derive_seed(cfg.seed, &[hash_str(&sc.id), r as u64])
}

pub fn column(vals: &[Values], k: &str) -> Vec<f64> {
    vals.iter().map(|v| v.get(k).copied().unwrap_or(0.0)).collect()
}

/// Half-width of the primary metric's interval relative to its scale.
pub fn precision_of(vals: &[Values], k: &str, conf: f64) -> f64 {
    precision_with(vals, k, conf, default_kind(k))
}

/// Half-width of a metric's interval: absolute for a bounded metric (a
/// score, a rate), relative to the value for everything else.
pub fn precision_with(vals: &[Values], k: &str, conf: f64, kind: MetricKind) -> f64 {
    precision_col(&column(vals, k), conf, kind)
}

/// `precision_with` for one metric's values.
pub fn precision_col(col: &[f64], conf: f64, kind: MetricKind) -> f64 {
    if col.len() < 2 {
        return f64::INFINITY;
    }
    let mut w = Welford::new();
    col.iter().filter(|x| !x.is_nan()).for_each(|&x| w.push(x));
    match kind {
        MetricKind::Bounded { per_rep } => {
            let (lo, hi) = bounded_ci(w.mean, w.var(), w.n as f64, per_rep, conf);
            (hi - lo) / 2.0
        }
        MetricKind::Median { rel_cell } => {
            let (m, lo, hi) = median_ci(col, conf);
            let (lo, hi) = grid_floor(m, lo, hi, m, rel_cell);
            if m.abs() > 1e-12 { (hi - lo) / 2.0 / m.abs() } else { f64::INFINITY }
        }
        MetricKind::Mean => {
            if w.mean.abs() < 1e-12 {
                return if w.sd() < 1e-12 { 0.0 } else { f64::INFINITY };
            }
            w.half_width(conf) / w.mean.abs()
        }
    }
}

/// Run every variant on the same replication seeds. Variants may come from
/// different data versions (`worlds`), e.g. before and after a patch; the
/// scenario is looked up by id in each variant's world.
pub fn run(worlds: &[&World], scenario: &str, variants: &[Variant], cfg: &McCfg) -> McResult {
    let scs: Vec<&ScenarioDef> = variants
        .iter()
        .map(|v| worlds[v.world].scenarios.get(scenario).expect("scenario missing in a compared data version"))
        .collect();
    let sc = scs[0];
    let threads = if cfg.threads == 0 {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    } else {
        cfg.threads
    };
    let mut values: Vec<Vec<Values>> = vec![Vec::new(); variants.len()];
    let mut done = 0usize;
    let mut converged = false;
    while done < cfg.max_reps {
        let n = cfg.batch.min(cfg.max_reps - done).max(1);
        let slots: Vec<Mutex<Option<Vec<Values>>>> = (0..n).map(|_| Mutex::new(None)).collect();
        let next = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..threads.min(n) {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let r = done + i;
                    let seed = rep_seed(cfg, sc, r);
                    let out: Vec<Values> = variants
                        .iter()
                        .enumerate()
                        .map(|(vi, v)| {
                            run_rep(&RepInput {
                                w: worlds[v.world],
                                sc: scs[vi],
                                build: &v.build,
                                opponent: v.opponent.as_ref(),
                                opts: &v.opts,
                                seed,
                                log: false,
                            })
                            .0
                        })
                        .collect();
                    *slots[i].lock().unwrap() = Some(out);
                });
            }
        });
        for slot in slots {
            let out = slot.into_inner().unwrap().expect("replication did not finish");
            for (vi, v) in out.into_iter().enumerate() {
                values[vi].push(v);
            }
        }
        done += n;
        if done >= cfg.min_reps {
            let ok = values.iter().enumerate().all(|(vi, vals)| {
                let v = &variants[vi];
                let kind = crate::scenario::metric_kind(worlds[v.world], scs[vi], &v.opts, &sc.primary);
                precision_with(vals, &sc.primary, cfg.conf, kind) <= cfg.precision
            });
            if ok {
                converged = true;
                break;
            }
        }
    }
    McResult { reps: done, converged, values }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MetricSummary {
    pub name: String,
    pub mean: f64,
    pub lo: f64,
    pub hi: f64,
    pub sd: f64,
    pub median: f64,
    pub p10: f64,
    pub p90: f64,
    pub kind: &'static str,
}

pub fn summarise(vals: &[Values], conf: f64) -> Vec<MetricSummary> {
    summarise_with(vals, conf, &default_kind)
}

/// Every metric's summary, each by its kind.
pub fn summarise_with(vals: &[Values], conf: f64, kind_of: &dyn Fn(&str) -> MetricKind) -> Vec<MetricSummary> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for v in vals {
        keys.extend(v.keys().cloned());
    }
    keys.into_iter().map(|k| {
        let col = column(vals, &k);
        summary_of(&k, &col, conf, kind_of(&k))
    }).collect()
}

/// One metric's summary from every replication's value, by its kind.
pub fn summary_of(k: &str, col: &[f64], conf: f64, kind: MetricKind) -> MetricSummary {
    {
        let mut w = Welford::new();
        col.iter().filter(|x| !x.is_nan()).for_each(|&x| w.push(x));
        let (lo, hi, kind) = match kind {
            MetricKind::Bounded { per_rep } => {
                let (lo, hi) = bounded_ci(w.mean, w.var(), w.n as f64, per_rep, conf);
                (lo, hi, "bounded")
            }
            MetricKind::Median { rel_cell } => {
                let (m, lo, hi) = median_ci(col, conf);
                let (lo, hi) = grid_floor(m, lo, hi, m, rel_cell);
                (lo, hi, "median")
            }
            MetricKind::Mean => {
                let h = w.half_width(conf);
                // a quantity that is never negative cannot have a negative lower bound
                let floor = if col.iter().all(|&x| x >= 0.0) { 0.0 } else { f64::NEG_INFINITY };
                ((w.mean - h).max(floor), w.mean + h, "mean")
            }
        };
        MetricSummary {
            name: k.to_string(),
            mean: w.mean,
            lo,
            hi,
            sd: w.sd(),
            median: quantile(col, 0.5),
            p10: quantile(col, 0.1),
            p90: quantile(col, 0.9),
            kind,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PairedSummary {
    pub metric: String,
    pub baseline: String,
    pub variant: String,
    /// Indices of `baseline` and `variant` among the compared variants.
    pub a: usize,
    pub b: usize,
    pub diff: f64,
    pub lo: f64,
    pub hi: f64,
    pub significant: bool,
    pub variance_reduction: f64,
    /// Replications in common, the difference's standard error, the
    /// baseline's own value, and how the difference was taken ("mean":
    /// paired t; "median": difference of medians, paired bootstrap).
    pub n: usize,
    pub se: f64,
    pub base: f64,
    pub method: &'static str,
}

/// Every variant against the first.
pub fn compare(names: &[String], res: &McResult, metrics: &[String], conf: f64) -> Vec<PairedSummary> {
    let pairs: Vec<(usize, usize)> = (1..res.values.len()).map(|v| (0, v)).collect();
    compare_pairs(names, res, metrics, &pairs, conf, &default_kind)
}

/// Variant `b` against variant `a` for each pair, on the same replications.
pub fn compare_pairs(names: &[String], res: &McResult, metrics: &[String], pairs: &[(usize, usize)], conf: f64, kind_of: &dyn Fn(&str) -> MetricKind) -> Vec<PairedSummary> {
    let mut out = Vec::new();
    for m in metrics {
        let kind = kind_of(m);
        for &(a, b) in pairs {
            out.push(pair_summary(m, names, a, b, &column(&res.values[a], m), &column(&res.values[b], m), conf, kind));
        }
    }
    out
}

/// Build `b` against build `a` on metric `m`, from their values.
#[allow(clippy::too_many_arguments)]
pub fn pair_summary(m: &str, names: &[String], a: usize, b: usize, col_a: &[f64], col_b: &[f64], conf: f64, kind: MetricKind) -> PairedSummary {
    let p = pair_stats(m, col_a, col_b, conf, kind);
    PairedSummary {
        metric: m.to_string(),
        baseline: names[a].clone(),
        variant: names[b].clone(),
        a,
        b,
        diff: p.diff,
        lo: p.lo,
        hi: p.hi,
        significant: p.lo > 0.0 || p.hi < 0.0,
        variance_reduction: p.variance_reduction,
        n: p.n,
        se: p.se,
        base: p.base,
        method: p.method,
    }
}

/// One paired comparison, `col` against `base`.
#[derive(Clone, Copy, Debug)]
pub struct PairStat {
    pub diff: f64,
    pub lo: f64,
    pub hi: f64,
    pub variance_reduction: f64,
    pub n: usize,
    pub se: f64,
    pub base: f64,
    pub method: &'static str,
}

/// The fixed part of the bootstrap seed: with the metric and the number of
/// replications, nothing about how the work was split.
const BOOT_SEED: u64 = 0x5eed_b007;

/// Difference `col - base` of metric `m` over replications run on the same
/// seeds. A median metric (a breakeven) compares the medians, med(col) −
/// med(base), with a paired bootstrap range at least one grid cell wide;
/// everything else the mean difference, with a paired t range.
pub fn pair_stats(m: &str, base: &[f64], col: &[f64], conf: f64, kind: MetricKind) -> PairStat {
    let p = paired(base, col, conf);
    let n = base.len().min(col.len());
    if let MetricKind::Median { rel_cell } = kind {
        let seed = derive_seed(BOOT_SEED, &[hash_str(m), n as u64]);
        let (diff, lo, hi) = paired_median_boot(base, col, conf, seed);
        let b = median_ci(base, conf).0;
        let (lo, hi) = grid_floor(diff, lo, hi, b, rel_cell);
        let z = norm_inv(1.0 - (1.0 - conf) / 2.0);
        PairStat { diff, lo, hi, variance_reduction: p.variance_reduction, n, se: (hi - lo) / (2.0 * z), base: b, method: "median" }
    } else {
        let mut w = Welford::new();
        base.iter().filter(|x| !x.is_nan()).for_each(|&x| w.push(x));
        let se = if p.n >= 2 { p.half_width / t_quantile(1.0 - (1.0 - conf) / 2.0, (p.n - 1) as f64) } else { f64::INFINITY };
        PairStat { diff: p.mean_diff, lo: p.mean_diff - p.half_width, hi: p.mean_diff + p.half_width, variance_reduction: p.variance_reduction, n, se, base: w.mean, method: "mean" }
    }
}
