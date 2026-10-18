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

pub fn rep_seed(cfg: &McCfg, sc: &ScenarioDef, r: usize) -> u64 {
    derive_seed(cfg.seed, &[hash_str(&sc.id), r as u64])
}

pub fn column(vals: &[Values], k: &str) -> Vec<f64> {
    vals.iter().map(|v| v.get(k).copied().unwrap_or(0.0)).collect()
}

/// Half-width of the primary metric's interval relative to its scale.
pub fn precision_of(vals: &[Values], k: &str, conf: f64) -> f64 {
    let col = column(vals, k);
    if col.len() < 2 {
        return f64::INFINITY;
    }
    if is_proportion(k) {
        let s: f64 = col.iter().sum();
        let (lo, hi) = wilson(s, col.len() as f64, conf);
        return (hi - lo) / 2.0;
    }
    if is_median(k) {
        let (m, lo, hi) = median_ci(&col, conf);
        return if m.abs() > 1e-12 { (hi - lo) / 2.0 / m.abs() } else { f64::INFINITY };
    }
    let mut w = Welford::new();
    col.iter().for_each(|&x| w.push(x));
    if is_score(k) {
        return w.half_width(conf);
    }
    if w.mean.abs() < 1e-12 {
        return if w.sd() < 1e-12 { 0.0 } else { f64::INFINITY };
    }
    w.half_width(conf) / w.mean.abs()
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
            let ok = values.iter().all(|vals| precision_of(vals, &sc.primary, cfg.conf) <= cfg.precision);
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
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for v in vals {
        keys.extend(v.keys().cloned());
    }
    let mut out = Vec::new();
    for k in keys {
        let col = column(vals, &k);
        let mut w = Welford::new();
        col.iter().for_each(|&x| w.push(x));
        let (lo, hi, kind) = if is_proportion(&k) {
            let (lo, hi) = wilson(col.iter().sum(), col.len() as f64, conf);
            (lo, hi, "proportion")
        } else if is_median(&k) {
            let (_, lo, hi) = median_ci(&col, conf);
            (lo, hi, "median")
        } else {
            let h = w.half_width(conf);
            // a quantity that is never negative cannot have a negative lower bound
            let floor = if col.iter().all(|&x| x >= 0.0) { 0.0 } else { f64::NEG_INFINITY };
            ((w.mean - h).max(floor), w.mean + h, "mean")
        };
        out.push(MetricSummary {
            name: k.clone(),
            mean: w.mean,
            lo,
            hi,
            sd: w.sd(),
            median: quantile(&col, 0.5),
            p10: quantile(&col, 0.1),
            p90: quantile(&col, 0.9),
            kind,
        });
    }
    out
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
}

/// Every variant against the first.
pub fn compare(names: &[String], res: &McResult, metrics: &[String], conf: f64) -> Vec<PairedSummary> {
    let pairs: Vec<(usize, usize)> = (1..res.values.len()).map(|v| (0, v)).collect();
    compare_pairs(names, res, metrics, &pairs, conf)
}

/// Variant `b` against variant `a` for each pair, on the same replications.
pub fn compare_pairs(names: &[String], res: &McResult, metrics: &[String], pairs: &[(usize, usize)], conf: f64) -> Vec<PairedSummary> {
    let mut out = Vec::new();
    for m in metrics {
        for &(a, b) in pairs {
            let (diff, lo, hi, variance_reduction) = pair_stats(m, &column(&res.values[a], m), &column(&res.values[b], m), conf);
            out.push(PairedSummary {
                metric: m.clone(),
                baseline: names[a].clone(),
                variant: names[b].clone(),
                a,
                b,
                diff,
                lo,
                hi,
                significant: lo > 0.0 || hi < 0.0,
                variance_reduction,
            });
        }
    }
    out
}

/// Difference `col - base` of metric `m` over replications run on the same
/// seeds: (difference, interval low, interval high, variance reduction).
/// Metrics summarised by their median are compared the same way: the median
/// of the paired differences, with a distribution-free interval. Everything
/// else: the mean difference.
pub fn pair_stats(m: &str, base: &[f64], col: &[f64], conf: f64) -> (f64, f64, f64, f64) {
    let p = paired(base, col, conf);
    if is_median(m) {
        let d: Vec<f64> = base.iter().zip(col).map(|(a, b)| b - a).collect();
        let (diff, lo, hi) = median_ci(&d, conf);
        (diff, lo, hi, p.variance_reduction)
    } else {
        (p.mean_diff, p.mean_diff - p.half_width, p.mean_diff + p.half_width, p.variance_reduction)
    }
}
