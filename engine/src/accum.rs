/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tdigest::TDigest;

pub trait Accum {
    type Input;
    type Output;
    fn insert(&mut self, input: Self::Input);
    fn merge(&mut self, other: &Self);
    fn extract(&self) -> Self::Output;
}

pub trait MergeAcc: Iterator {
    type Output;
    fn merge(self) -> Self::Output;
}

impl<'a, T, Acc> MergeAcc for T
where
    T: Iterator<Item = &'a Acc>,
    Acc: Accum + Default + 'a,
{
    type Output = Acc;

    fn merge(self) -> Self::Output {
        self.fold(Acc::default(), |mut sum, acc| {
            sum.merge(acc);
            sum
        })
    }
}

pub struct AccByKey<K, Acc>(pub BTreeMap<K, Acc>);

impl<K, Acc> Default for AccByKey<K, Acc> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<K: Ord + Clone, Acc: Accum + Default> Accum for AccByKey<K, Acc> {
    type Input = (K, Acc::Input);
    type Output = BTreeMap<K, Acc::Output>;

    fn insert(&mut self, (key, input): Self::Input) {
        let acc = self.0.entry(key).or_default();
        acc.insert(input)
    }

    fn merge(&mut self, other: &Self) {
        other.0.iter().for_each(|(key, acc)| {
            self.0.entry(key.clone()).or_default().merge(acc);
        });
    }

    fn extract(&self) -> BTreeMap<K, Acc::Output> {
        self.0
            .iter()
            .map(|(key, acc)| (key.clone(), acc.extract()))
            .collect()
    }
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Count(u64);

impl Accum for Count {
    type Input = ();
    type Output = u64;

    fn insert(&mut self, _input: ()) {
        self.0 += 1;
    }

    fn merge(&mut self, other: &Self) {
        self.0 += other.0;
    }

    fn extract(&self) -> Self::Output {
        self.0
    }
}

impl Accum for TDigest {
    type Input = f64;
    type Output = Self;

    fn insert(&mut self, v: f64) {
        *self = self.merge_sorted(vec![v])
    }

    fn merge(&mut self, other: &Self) {
        *self = TDigest::merge_digests(vec![self.clone(), other.clone()])
    }

    fn extract(&self) -> Self::Output {
        self.clone()
    }
}
