//! A session's results, kept by column. The metrics compared fight by fight
//! (the unprefixed ones: the primary, `dps`, `deaths`...) keep every value;
//! the per-ability and per-opponent breakdowns (`name:detail`) keep running
//! moments, which is all a mean and its range need. Rankings run dozens of
//! builds for thousands of fights, so this is the difference between
//! hundreds of megabytes and about one.

use crate::mc::{MetricKind, MetricSummary};
use crate::scenario::Values;
use crate::stats::*;
use std::collections::HashMap;

/// The metric names a session has seen, interned, and which keep every value.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    pub names: Vec<String>,
    index: HashMap<String, usize>,
    full: Vec<bool>,
    /// Every metric keeps every value (the `full` start option).
    all_full: bool,
    /// Only these metrics are kept (exact names, or prefixes ending in ':');
    /// the primary always is. None keeps everything.
    keep: Option<Vec<String>>,
    primary: String,
}

impl Registry {
    pub fn new(primary: &str, keep: Option<Vec<String>>, all_full: bool) -> Self {
        Registry { primary: primary.to_string(), keep, all_full, ..Default::default() }
    }

    /// Whether a metric is kept at all.
    pub fn keeps(&self, name: &str) -> bool {
        name == self.primary
            || self.keep.as_ref().map_or(true, |k| k.iter().any(|x| x == name || (x.ends_with(':') && name.starts_with(x.as_str()))))
    }

    pub fn get(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    /// The metric's slot, made on first sight.
    fn id(&mut self, name: &str) -> usize {
        if let Some(&i) = self.index.get(name) {
            return i;
        }
        let i = self.names.len();
        self.names.push(name.to_string());
        self.index.insert(name.to_string(), i);
        self.full.push(self.all_full || !name.contains(':'));
        i
    }

    pub fn is_full(&self, i: usize) -> bool {
        self.full[i]
    }
}

#[derive(Clone, Debug)]
pub enum Column {
    Full(Vec<f64>),
    Moments(Welford),
}

/// One build's replications, by registry slot.
#[derive(Clone, Debug, Default)]
pub struct Store {
    /// Replications stored.
    pub n: usize,
    cols: Vec<Option<Column>>,
}

impl Store {
    /// Add one replication. A metric missing from it counts as 0, and one
    /// first seen now counts as 0 in every replication before (as a column
    /// of the old maps read it), which running moments keep exactly.
    pub fn push(&mut self, reg: &mut Registry, v: &Values) {
        let mut seen = vec![false; self.cols.len()];
        for (k, &x) in v {
            if !reg.keeps(k) {
                continue;
            }
            let i = reg.id(k);
            if i >= self.cols.len() {
                self.cols.resize(i + 1, None);
                seen.resize(i + 1, false);
            }
            let n = self.n;
            let col = self.cols[i].get_or_insert_with(|| if reg.is_full(i) { Column::Full(vec![0.0; n]) } else { Column::Moments(zeros(n)) });
            match col {
                Column::Full(c) => c.push(x),
                // a value that isn't a number has no moments
                Column::Moments(w) => {
                    if !x.is_nan() {
                        w.push(x)
                    }
                }
            }
            seen[i] = true;
        }
        for (i, col) in self.cols.iter_mut().enumerate() {
            if seen.get(i).copied().unwrap_or(false) {
                continue;
            }
            match col {
                Some(Column::Full(c)) => c.push(0.0),
                Some(Column::Moments(w)) => w.push(0.0),
                None => {}
            }
        }
        self.n += 1;
    }

    /// Every value of a metric that keeps them (zeros if it never came up).
    pub fn column(&self, reg: &Registry, name: &str) -> Option<Vec<f64>> {
        match reg.get(name).and_then(|i| self.cols.get(i)).and_then(|c| c.as_ref()) {
            Some(Column::Full(c)) => Some(c.clone()),
            Some(Column::Moments(_)) => None,
            None if reg.get(name).map_or(true, |i| reg.is_full(i)) => Some(vec![0.0; self.n]),
            None => None,
        }
    }

    /// A metric's column without copying (None for moments or unseen).
    pub fn full(&self, reg: &Registry, name: &str) -> Option<&[f64]> {
        match reg.get(name).and_then(|i| self.cols.get(i)).and_then(|c| c.as_ref()) {
            Some(Column::Full(c)) => Some(c),
            _ => None,
        }
    }

    /// Running moments of any metric (a full column's are worked out).
    pub fn moments(&self, reg: &Registry, name: &str) -> Welford {
        match reg.get(name).and_then(|i| self.cols.get(i)).and_then(|c| c.as_ref()) {
            Some(Column::Full(c)) => {
                let mut w = Welford::new();
                c.iter().filter(|x| !x.is_nan()).for_each(|&x| w.push(x));
                w
            }
            Some(Column::Moments(w)) => w.clone(),
            None => zeros(self.n),
        }
    }

    /// Every metric this build has a value for, summarised by its kind:
    /// exactly what `mc::summarise_with` gives the same replications for the
    /// metrics that keep their values; for the moments, the mean and its
    /// range (the median stands at the mean; no deciles).
    pub fn summarise(&self, reg: &Registry, conf: f64, kind_of: &dyn Fn(&str) -> MetricKind) -> Vec<MetricSummary> {
        let mut order: Vec<usize> = (0..reg.names.len()).filter(|&i| self.cols.get(i).map_or(false, |c| c.is_some())).collect();
        order.sort_by(|a, b| reg.names[*a].cmp(&reg.names[*b]));
        let mut out = Vec::with_capacity(order.len());
        for i in order {
            let name = &reg.names[i];
            match self.cols[i].as_ref().unwrap() {
                Column::Full(col) => out.push(crate::mc::summary_of(name, col, conf, kind_of(name))),
                Column::Moments(w) => {
                    let (lo, hi, kind) = match kind_of(name) {
                        MetricKind::Bounded { per_rep } => {
                            let (lo, hi) = bounded_ci(w.mean, w.var(), w.n as f64, per_rep, conf);
                            (lo, hi, "bounded")
                        }
                        _ => {
                            let h = w.half_width(conf);
                            let floor = if w.min >= 0.0 { 0.0 } else { f64::NEG_INFINITY };
                            ((w.mean - h).max(floor), w.mean + h, "mean")
                        }
                    };
                    out.push(MetricSummary { name: name.clone(), mean: w.mean, lo, hi, sd: w.sd(), median: w.mean, p10: f64::NAN, p90: f64::NAN, kind });
                }
            }
        }
        out
    }
}

/// Running moments of `n` zeros.
fn zeros(n: usize) -> Welford {
    let mut w = Welford::new();
    for _ in 0..n {
        w.push(0.0);
    }
    w
}

/// Half-width of a metric's range from its running moments: absolute for a
/// bounded metric, relative to the mean otherwise.
pub fn precision_of_moments(w: &Welford, kind: MetricKind, conf: f64) -> f64 {
    if w.n < 2 {
        return f64::INFINITY;
    }
    match kind {
        MetricKind::Bounded { per_rep } => {
            let (lo, hi) = bounded_ci(w.mean, w.var(), w.n as f64, per_rep, conf);
            (hi - lo) / 2.0
        }
        _ => {
            if w.mean.abs() < 1e-12 {
                return if w.sd() < 1e-12 { 0.0 } else { f64::INFINITY };
            }
            w.half_width(conf) / w.mean.abs()
        }
    }
}
