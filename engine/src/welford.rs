/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::marker::PhantomData;

use rustc_apfloat::{ieee::Double, Float, FloatConvert};
use serde::{
    de::{IgnoredAny, Visitor},
    ser::SerializeStruct,
    Deserialize, Serialize, Serializer,
};

use crate::{accum::Accum, window::Window};

#[derive(Clone, Default, Debug)]
pub struct Welford<T> {
    pub count: T,
    pub mean: T,
    pub m2: T,
}

impl<T> Accum for Welford<T>
where
    T: Float + FloatConvert<Double>,
    Double: FloatConvert<T>,
{
    type Input = f64;
    type Output = Welford<f64>;

    fn insert(&mut self, n: f64) {
        let n = from_f64(n);
        let old_mean = self.mean;
        self.count += from_f64(1.0);
        self.mean += ((n - old_mean).value / self.count).value;
        self.m2 = (n - self.mean)
            .value
            .mul_add((n - old_mean).value, self.m2)
            .value;
    }

    fn merge(&mut self, other: &Self) {
        let delta = (other.mean - self.mean).value;
        self.mean += delta
            .mul_add((other.count / self.count).value, other.count)
            .value;
        self.m2 += (delta * delta)
            .value
            .mul_add(
                ((self.count * other.count).value / (self.count + other.count).value).value,
                other.m2,
            )
            .value;
        self.count += other.count;
    }

    fn extract(&self) -> Self::Output {
        Welford {
            count: to_f64(self.count),
            mean: to_f64(self.mean),
            m2: to_f64(self.m2),
        }
    }
}

pub(crate) fn from_f64<T>(n: f64) -> T
where
    T: Float,
    Double: FloatConvert<T>,
{
    Double::from_bits(n.to_bits() as u128)
        .convert(&mut false)
        .value
}

pub(crate) fn to_f64<T>(n: T) -> f64
where
    T: Float + FloatConvert<Double>,
{
    f64::from_bits(n.convert(&mut false).value.to_bits() as u64)
}

impl<T> Window<Welford<T>>
where
    T: Float + FloatConvert<Double>,
    Double: FloatConvert<T>,
{
    const fn first_last(&self) -> (&Welford<T>, &Welford<T>) {
        (self.first(), self.current())
    }

    pub fn count(&self) -> T {
        let (a, ab) = self.first_last();
        (ab.count - a.count).value
    }

    pub fn mean(&self) -> T {
        let (a, ab) = self.first_last();
        (a.mean + ((ab.mean - a.mean).value * (ab.count / (ab.count - a.count).value).value).value)
            .value
    }

    pub fn m2(&self) -> T {
        let (a, ab) = self.first_last();
        let count = (ab.count - a.count).value;
        let mean_diff = (ab.mean - a.mean).value;
        ((ab.m2 - a.m2).value
            - ((mean_diff * mean_diff).value * ((ab.count * a.count).value / count).value).value)
            .value
    }

    pub fn stddev(&self) -> T {
        let df = (self.count() - from_f64(1.0)).value;
        let var = (self.m2() / df).value;
        T::from_bits(
            ieee_apsqrt::sqrt_fast(var.to_bits(), rustc_apfloat::Round::NearestTiesToEven)
                .0
                .value,
        )
    }

    pub fn confidence_interval(&self, q: f64) -> T {
        let count = self.count();
        let df = (count - from_f64(1.0)).value;
        ((self.stddev() * from_f64(distrs::StudentsT::cdf(q, to_f64(df)))).value / count).value
    }

    pub fn lower_bound_of_confidence_interval(&self, q: f64) -> T {
        (self.mean() - self.confidence_interval(q)).value
    }

    pub fn upper_bound_of_confidence_interval(&self, q: f64) -> T {
        (self.mean() + self.confidence_interval(q)).value
    }
}

impl<T: Float> Serialize for Welford<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("Welford", 3)?;
        s.serialize_field("count", &self.count.to_bits())?;
        s.serialize_field("mean", &self.mean.to_bits())?;
        s.serialize_field("m2", &self.m2.to_bits())?;
        s.end()
    }
}

impl<'de, T: Float> Deserialize<'de> for Welford<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct WelfordVisitor<T>(PhantomData<T>);

        impl<'de, T: Float> Visitor<'de> for WelfordVisitor<T> {
            type Value = Welford<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "struct Welford")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let (mut count, mut mean, mut m2) = (None, None, None);

                while let Some(field) = map.next_key::<String>()? {
                    match field.as_str() {
                        "count" => {
                            count = Some(T::from_bits(map.next_value()?));
                        }
                        "mean" => {
                            mean = Some(T::from_bits(map.next_value()?));
                        }
                        "m2" => {
                            m2 = Some(T::from_bits(map.next_value()?));
                        }
                        _ => {
                            let _ = map.next_value::<IgnoredAny>()?;
                        }
                    }
                }

                let count = count.ok_or_else(|| {
                    <A::Error as serde::de::Error>::custom("missing field 'count'")
                })?;
                let mean = mean.ok_or_else(|| {
                    <A::Error as serde::de::Error>::custom("missing field 'mean'")
                })?;
                let m2 =
                    m2.ok_or_else(|| <A::Error as serde::de::Error>::custom("missing field 'm2'"))?;

                Ok(Welford { count, mean, m2 })
            }
        }

        deserializer.deserialize_struct(
            "Welford",
            &["count", "mean", "m2"],
            WelfordVisitor(PhantomData),
        )
    }
}
