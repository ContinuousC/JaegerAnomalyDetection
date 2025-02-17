/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use jaeger_anomaly_detection::{Duration, WindowConfig};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Window<T> {
    i: usize,
    start: DateTime<Utc>,
    bin_width: Duration,
    ring: Box<[T]>,
}

impl<T: Default> Window<T> {
    pub fn new(start: DateTime<Utc>, config: &WindowConfig) -> Self {
        Self::new_init(start, |_| T::default(), config)
    }

    // pub fn advance(&mut self, t: DateTime<Utc>) {
    //     self.advance_init(t, |_| T::default());
    // }

    pub fn advance_with<'a, F, U>(
        &'a mut self,
        t: DateTime<Utc>,
        output: F,
    ) -> impl Iterator<Item = U> + 'a
    where
        F: FnMut(&Self) -> U + 'a,
    {
        self.advance_with_init(t, |_| T::default(), output)
    }
}

impl<T> Window<T> {
    pub fn new_init<F>(start: DateTime<Utc>, mut init: F, config: &WindowConfig) -> Self
    where
        F: FnMut(DateTime<Utc>) -> T,
    {
        let start = start
            .duration_trunc(config.bin_width.to_time_delta())
            .unwrap();
        let bin_width = config.bin_width.to_time_delta();
        Window {
            i: 0,
            start,
            bin_width: config.bin_width,
            ring: (0..config.num_bins)
                .map(|i| init(start + bin_width * i as i32))
                .collect(),
        }
    }

    pub fn advance_init<F>(&mut self, t: DateTime<Utc>, mut init: F)
    where
        F: FnMut(DateTime<Utc>) -> T,
    {
        let t = t.duration_trunc(self.bin_width()).unwrap();
        // T can regress from one query to the next if traces are
        // written out-of-order.
        // assert!(t >= self.start);
        loop {
            let next = self.start + self.bin_width();
            if next <= t {
                self.i = (self.i + 1) % self.ring.len();
                self.ring[self.i] = init(next);
                self.start = next;
            } else {
                break;
            }
        }
    }

    pub fn advance_with_init<'a, F, G, U>(
        &'a mut self,
        t: DateTime<Utc>,
        mut init: F,
        mut output: G,
    ) -> impl Iterator<Item = U> + 'a
    where
        F: FnMut(DateTime<Utc>) -> T + 'a,
        G: FnMut(&Self) -> U + 'a,
    {
        let t = t.duration_trunc(self.bin_width()).unwrap();
        // T can regress from one query to the next if traces are
        // written out-of-order.
        // assert!(t >= self.start);
        std::iter::from_fn(move || {
            let next = self.start + self.bin_width();
            if next <= t {
                let value = output(&*self);
                self.i = (self.i + 1) % self.ring.len();
                self.ring[self.i] = init(next);
                self.start = next;
                Some(value)
            } else {
                None
            }
        })
    }

    // pub fn start(&self) -> DateTime<Utc> {
    //     self.start
    // }

    // pub fn end(&self) -> DateTime<Utc> {
    //     self.start + self.bin_width
    // }

    pub const fn bin_width(&self) -> TimeDelta {
        self.bin_width.to_time_delta()
    }

    pub const fn num_bins(&self) -> usize {
        self.ring.len()
    }

    pub const fn minutes(&self) -> f64 {
        self.bin_width.multiply(self.ring.len() as u32).minutes()
    }

    pub fn compatible_with(&self, config: &WindowConfig) -> bool {
        self.bin_width() == config.bin_width.to_time_delta() && self.num_bins() == config.num_bins
    }

    pub const fn current(&self) -> &T {
        &self.ring[self.i]
    }

    pub const fn first(&self) -> &T {
        &self.ring[(self.i + 1) % self.ring.len()]
    }

    pub fn current_mut(&mut self) -> &mut T {
        &mut self.ring[self.i]
    }

    pub fn bins(&self) -> impl Iterator<Item = &T> {
        let first = self.i + 1 % self.ring.len();
        self.ring[first..].iter().chain(&self.ring[..first])
    }
}
