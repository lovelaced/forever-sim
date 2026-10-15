//! Statistics for Monte Carlo output: running moments, confidence intervals
//! (Student-t for means, Wilson for proportions, distribution-free for
//! medians) and paired differences for common-random-number comparisons.

/// Welford's online mean/variance.
#[derive(Clone, Debug, Default)]
pub struct Welford {
    pub n: u64,
    pub mean: f64,
    m2: f64,
    pub min: f64,
    pub max: f64,
}

impl Welford {
    pub fn new() -> Self {
        Welford { n: 0, mean: 0.0, m2: 0.0, min: f64::INFINITY, max: f64::NEG_INFINITY }
    }
    pub fn push(&mut self, x: f64) {
        self.n += 1;
        let d = x - self.mean;
        self.mean += d / self.n as f64;
        self.m2 += d * (x - self.mean);
        if x < self.min {
            self.min = x;
        }
        if x > self.max {
            self.max = x;
        }
    }
    pub fn var(&self) -> f64 {
        if self.n < 2 {
            0.0
        } else {
            self.m2 / (self.n - 1) as f64
        }
    }
    pub fn sd(&self) -> f64 {
        self.var().sqrt()
    }
    pub fn se(&self) -> f64 {
        if self.n == 0 {
            f64::INFINITY
        } else {
            self.sd() / (self.n as f64).sqrt()
        }
    }
    /// Half-width of the two-sided confidence interval for the mean.
    pub fn half_width(&self, conf: f64) -> f64 {
        if self.n < 2 {
            return f64::INFINITY;
        }
        t_quantile(1.0 - (1.0 - conf) / 2.0, (self.n - 1) as f64) * self.se()
    }
}

/// Inverse of the standard normal CDF (Acklam's rational approximation,
/// relative error < 1.2e-9).
pub fn norm_inv(p: f64) -> f64 {
    assert!(p > 0.0 && p < 1.0, "norm_inv({p})");
    const A: [f64; 6] = [
        -3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02,
        1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02,
        6.680131188771972e+01, -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00,
        -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let pl = 0.02425;
    if p < pl {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - pl {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

/// Student-t quantile via the Cornish-Fisher expansion around the normal
/// quantile (accurate to ~1e-3 for df >= 3, which is all we use it for).
pub fn t_quantile(p: f64, df: f64) -> f64 {
    let z = norm_inv(p);
    if df > 1e6 {
        return z;
    }
    let z3 = z * z * z;
    let z5 = z3 * z * z;
    let z7 = z5 * z * z;
    z + (z3 + z) / (4.0 * df)
        + (5.0 * z5 + 16.0 * z3 + 3.0 * z) / (96.0 * df * df)
        + (3.0 * z7 + 19.0 * z5 + 17.0 * z3 - 15.0 * z) / (384.0 * df * df * df)
}

/// Wilson score interval for a binomial proportion. Returns (low, high).
pub fn wilson(successes: f64, n: f64, conf: f64) -> (f64, f64) {
    if n <= 0.0 {
        return (0.0, 1.0);
    }
    let z = norm_inv(1.0 - (1.0 - conf) / 2.0);
    let p = successes / n;
    let denom = 1.0 + z * z / n;
    let centre = (p + z * z / (2.0 * n)) / denom;
    let half = z * ((p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt()) / denom;
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

/// Median with a distribution-free confidence interval from order
/// statistics (normal approximation to the binomial ranks).
pub fn median_ci(values: &[f64], conf: f64) -> (f64, f64, f64) {
    if values.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    let mut v: Vec<f64> = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    let med = if n % 2 == 1 { v[n / 2] } else { 0.5 * (v[n / 2 - 1] + v[n / 2]) };
    let z = norm_inv(1.0 - (1.0 - conf) / 2.0);
    let nf = n as f64;
    let lo = ((nf / 2.0 - z * nf.sqrt() / 2.0).floor() as isize - 1).max(0) as usize;
    let hi = ((nf / 2.0 + z * nf.sqrt() / 2.0).ceil() as usize).min(n - 1);
    (med, v[lo], v[hi])
}

pub fn quantile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut v: Vec<f64> = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pos = q.clamp(0.0, 1.0) * (v.len() - 1) as f64;
    let i = pos.floor() as usize;
    let f = pos - i as f64;
    if i + 1 < v.len() {
        v[i] * (1.0 - f) + v[i + 1] * f
    } else {
        v[i]
    }
}

/// Summary of a paired comparison (same replication seeds for A and B).
#[derive(Clone, Debug)]
pub struct Paired {
    pub mean_diff: f64,
    pub half_width: f64,
    pub n: u64,
    /// Var(A)+Var(B) over Var(A-B): how much CRN pairing tightened the CI.
    pub variance_reduction: f64,
}

pub fn paired(a: &[f64], b: &[f64], conf: f64) -> Paired {
    let n = a.len().min(b.len());
    let mut d = Welford::new();
    let mut wa = Welford::new();
    let mut wb = Welford::new();
    for i in 0..n {
        d.push(b[i] - a[i]);
        wa.push(a[i]);
        wb.push(b[i]);
    }
    let vr = if d.var() > 0.0 { (wa.var() + wb.var()) / d.var() } else { f64::INFINITY };
    Paired { mean_diff: d.mean, half_width: d.half_width(conf), n: d.n, variance_reduction: vr }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    /// The intervals the simulator reports should cover the truth about as
    /// often as they claim. Checked on skewed data (exponential, like kill
    /// times) and on rare events (like deaths).
    #[test]
    fn interval_coverage() {
        let mut rng = Rng::new(99);
        let (trials, n) = (4000, 40);
        let mut hit_mean = 0;
        let mut hit_p = 0;
        let mut hit_med = 0;
        let ln2 = std::f64::consts::LN_2;
        for _ in 0..trials {
            let mut w = Welford::new();
            let mut xs = Vec::with_capacity(n);
            let mut k = 0.0;
            for _ in 0..n {
                let x = -(1.0 - rng.f64()).ln(); // exponential, mean 1, median ln 2
                w.push(x);
                xs.push(x);
                if rng.chance(0.1) {
                    k += 1.0;
                }
            }
            let h = w.half_width(0.95);
            if (w.mean - 1.0).abs() <= h {
                hit_mean += 1;
            }
            let (lo, hi) = wilson(k, n as f64, 0.95);
            if lo <= 0.1 && 0.1 <= hi {
                hit_p += 1;
            }
            let (_, mlo, mhi) = median_ci(&xs, 0.95);
            if mlo <= ln2 && ln2 <= mhi {
                hit_med += 1;
            }
        }
        let c = |h: i32| h as f64 / trials as f64;
        assert!((0.92..0.97).contains(&c(hit_mean)), "t interval coverage {}", c(hit_mean));
        assert!((0.93..0.99).contains(&c(hit_p)), "Wilson coverage {}", c(hit_p));
        assert!((0.93..0.995).contains(&c(hit_med)), "median interval coverage {}", c(hit_med));
    }

    #[test]
    fn normal_quantiles() {
        assert!((norm_inv(0.975) - 1.959964).abs() < 1e-5);
        assert!((norm_inv(0.5)).abs() < 1e-9);
        assert!((norm_inv(0.01) + 2.326348).abs() < 1e-5);
    }

    #[test]
    fn t_quantiles() {
        // reference values: t(0.975, 10)=2.228139, t(0.975, 30)=2.042272
        assert!((t_quantile(0.975, 10.0) - 2.228139).abs() < 2e-3);
        assert!((t_quantile(0.975, 30.0) - 2.042272).abs() < 1e-3);
    }

    #[test]
    fn welford_matches_two_pass() {
        let xs = [3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0];
        let mut w = Welford::new();
        xs.iter().for_each(|&x| w.push(x));
        let m = xs.iter().sum::<f64>() / xs.len() as f64;
        let v = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64;
        assert!((w.mean - m).abs() < 1e-12);
        assert!((w.var() - v).abs() < 1e-12);
    }

    #[test]
    fn wilson_known() {
        // 50/100 at 95%: (0.4038, 0.5962)
        let (lo, hi) = wilson(50.0, 100.0, 0.95);
        assert!((lo - 0.4038).abs() < 1e-3 && (hi - 0.5962).abs() < 1e-3);
    }

    #[test]
    fn median_interval_contains_median() {
        let v: Vec<f64> = (0..101).map(|i| i as f64).collect();
        let (m, lo, hi) = median_ci(&v, 0.95);
        assert_eq!(m, 50.0);
        assert!(lo < 50.0 && hi > 50.0 && lo >= 38.0 && hi <= 62.0);
    }
}
