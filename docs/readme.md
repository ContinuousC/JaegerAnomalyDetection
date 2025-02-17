# Jaeger Anomaly Detection

The Jaeger Anomaly Detection daemon reads trace data from Opensearch
and calculates statistical variables over time on the metrics. The
results are written to Cortex via the Prometheus Remove Write
protocol.

Among other statistics produced, the Anomaly Score on a metric
reflects the measure in which the value of a metric over the short
term (immediate interval) lies within the variability of the metric
over the long term (reference interval). It is expressed as a factor,
with values of 1 and below indicating a "normal" situation (current
value equal or below the reference), and values (much) higher than 1
indicating an abnormality (current value (at least) x times higher
than "normal").

For more information on the inner workings of the daemon, check the
[analysis document](Application%20Alerting.pptx).

## Repo structure

The repository is split up between a lib crate, containing code used
in other projects, and an engine crate that depends on the lib crate,
containing the code for the daemon itself.

The library crate contains config types (`config.rs`), enums used to
represent the intervals for pre-calculated anomaly scores
(`anomaly_score.rs`) and a structure used to generate expressions for
the metrics based on [Welford's online
algorithm](https://en.wikipedia.org/wiki/Algorithms_for_calculating_variance#Welford's_online_algorithm)
(`exprs.rs`).
