/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::{Add, Div, Mul, Sub};
use std::sync::{LazyLock, Mutex};

use ordered_float::OrderedFloat;
use prometheus_api::GenericLabels;
use prometheus_core::{LabelName, MetricName};
use prometheus_expr::{Expr, LabelSelector, MetricSelector, Offset, PromDuration};
use serde::{Deserialize, Serialize};
use statrs::distribution::{ContinuousCDF, Normal, StudentsT};
use tap::Pipe;

#[cfg_attr(feature = "apistos", derive(apistos::ApiComponent))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct WelfordParams {
    pub metric: MetricName,
    pub labels: GenericLabels,
    // TODO:
    pub group_by: Option<Vec<LabelName>>,
    pub duration: PromDuration,
    pub q: f64,
    pub labels_selectors: BTreeMap<LabelName, prometheus_schema::LabelSelector>,
}

#[cfg_attr(feature = "apistos", derive(apistos::ApiComponent))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct WelfordExprs {
    pub count: Expr,
    pub mean: Expr,
    pub stddev: Expr,
    pub confidence_interval: Expr,
    pub low: Expr,
    pub high: Expr,
}

impl WelfordExprs {
    pub fn new(
        WelfordParams {
            metric,
            labels,
            group_by,
            duration,
            q,
            labels_selectors,
        }: &WelfordParams,
    ) -> Self {
        let query = || {
            std::iter::once((
                LabelName::new_static("metric_type"),
                LabelSelector::Eq(String::from("welford")),
            ))
            .chain(
                labels
                    .iter()
                    .map(|(label, value)| (label.clone(), LabelSelector::Eq(value.clone()))),
            )
            .chain(
                labels_selectors
                    .iter()
                    .map(|(label, selector)| (label.clone(), selector.clone().into())),
            )
        };

        let count = MetricSelector::new()
            .metric(prometheus_core::MetricName::new(format!("trace_{metric}_count")).unwrap())
            .labels(query());
        let mean = MetricSelector::new()
            .metric(prometheus_core::MetricName::new(format!("trace_{metric}_mean")).unwrap())
            .labels(query());
        let m2 = MetricSelector::new()
            .metric(prometheus_core::MetricName::new(format!("trace_{metric}_m2")).unwrap())
            .labels(query());

        let offset = Offset::Positive(*duration);
        let counts = Expr::metric(count.clone()).sub(Expr::metric_offset(count.clone(), offset));
        let means = Expr::metric_offset(mean.clone(), offset).add(
            Expr::metric(mean.clone())
                .sub(Expr::metric_offset(mean.clone(), offset))
                .mul(Expr::metric(count.clone()).div(counts.clone().is_gt(Expr::number(0.0)))),
        );

        let count_over_time = counts
            .clone()
            .pipe(|expr| {
                if let Some(labels) = group_by {
                    expr.sum_by(labels.clone())
                } else {
                    expr
                }
            })
            .clamp_min(0.0);

        let df_over_time = counts
            .clone()
            .pipe(|expr| {
                if let Some(labels) = group_by {
                    expr.sum_by(labels.clone())
                } else {
                    expr
                }
            })
            .sub(1.0)
            .is_gt(0.0);

        let mean_over_time = means
            .clone()
            .pipe(|expr| {
                if let Some(labels) = group_by {
                    expr.mul(counts.clone())
                        .sum_by(labels.clone())
                        .div(counts.clone().sum_by(labels.clone()))
                } else {
                    expr
                }
            })
            .clamp_min(0.0);

        let stddev_over_time = Expr::metric(m2.clone())
            .sub(Expr::metric_offset(m2.clone(), offset))
            .sub(
                Expr::metric(mean.clone())
                    .sub(Expr::metric_offset(mean.clone(), offset))
                    .pow(2.0)
                    .mul(
                        Expr::metric(count.clone())
                            .mul(Expr::metric_offset(count.clone(), offset))
                            .div(counts.clone()),
                    ),
            )
            .pipe(|expr| {
                // To be verified...
                if let Some(labels) = group_by {
                    expr.add(counts.clone().mul(means.clone().pow(2.0)))
                        .sum_by(labels.clone())
                        .sub(
                            means
                                .clone()
                                .mul(counts.clone())
                                .sum_by(labels.clone())
                                .pow(2.0)
                                .div(counts.clone().sum_by(labels.clone())),
                        )
                } else {
                    expr
                }
            })
            .div(df_over_time.clone())
            .clamp_min(0.0)
            .pow(0.5);

        let confidence_interval = studentst_approx(*q, df_over_time)
            .mul(stddev_over_time.clone())
            .div(count_over_time.clone().pow(0.5));
        let low = mean_over_time
            .clone()
            .sub(confidence_interval.clone())
            .clamp_min(0.0);
        let high = mean_over_time.clone().add(confidence_interval.clone());
        Self {
            count: count_over_time,
            mean: mean_over_time.clone(),
            stddev: stddev_over_time,
            confidence_interval,
            low,
            high,
        }
    }
}

/* Approximate qt(q, df) for fixed q, variable df.
 *
 * R session:
 * > x <- 1 + c(0:5000) / 100
 * > q <- .99
 *
 * Plot error:
 * > plot(function(s) { lapply(s, function(s) {max((qnorm(q) + (qt(q, s + 1) - qnorm(q)) / (x-s) - qt(q, x)) / qt(q,x))}) }, from=0, to=1)
 *
 * Plot result:
 * > s <- .827141
 * > plot(x, qt(q, x), type="l", col="red"); lines(x, qnorm(q) + (qt(q, s + 1) - qnorm(q)) / (x-s))
 */
fn studentst_approx(q: f64, df_over_time: Expr) -> Expr {
    type State = BTreeMap<OrderedFloat<f64>, (f64, f64, f64)>;
    static CACHE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

    let (n, m, s) = *CACHE
        .lock()
        .unwrap()
        .entry(OrderedFloat(q))
        .or_insert_with(|| {
            let target = (1..=50)
                .map(|df| StudentsT::new(0.0, 1.0, df as f64).unwrap().inverse_cdf(q))
                .collect::<Vec<_>>();

            let n = Normal::new(0.0, 1.0).unwrap().inverse_cdf(q);
            let p = 100000;
            let (s, _e) = (0..p)
                .map(|s_| s_ as f64 / p as f64)
                .map(|s_| {
                    let t_ = StudentsT::new(0.0, 1.0, s_ + 1.0).unwrap().inverse_cdf(q);
                    let e = target
                        .iter()
                        .enumerate()
                        .map(|(i, y)| {
                            let df = (i + 1) as f64;
                            let y_ = n + (t_ - n) / (df - s_);
                            ((y - y_) / y).abs()
                        })
                        .max_by(|a, b| {
                            a.partial_cmp(b).unwrap_or_else(|| {
                                if a.is_nan() {
                                    Ordering::Less
                                } else {
                                    Ordering::Greater
                                }
                            })
                        })
                        .unwrap();
                    //.sum::<f64>();
                    (s_, e)
                })
                .min_by(|(_, a), (_, b)| {
                    a.partial_cmp(b).unwrap_or_else(|| {
                        if a.is_nan() {
                            Ordering::Greater
                        } else {
                            Ordering::Less
                        }
                    })
                })
                .unwrap();

            let t = StudentsT::new(0.0, 1.0, s + 1.0).unwrap().inverse_cdf(q);
            let m = t - n;

            // eprintln!("qt'({q}, df) = {n} + {m} / (df - {s})");

            // eprintln!("max(|(qt(q, df) - qt'(q, df)) / qt(q, df)|, df) = {e}");
            //eprintln!("sum(e^2) = {e}");

            // for (i, y) in target.iter().enumerate() {
            //     let df = i as f64 + 1.0;
            //     eprintln!(
            //         "error @ {df} = {:.2}%",
            //         (y - (n + m / (df - s))) / y * 100.0
            //     );
            // }

            (n, m, s)
        });

    Expr::number(n).add(Expr::number(m).div(df_over_time.sub(Expr::number(s))))
}
