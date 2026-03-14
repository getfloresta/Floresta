// SPDX-License-Identifier: MIT OR Apache-2.0

use core::num::NonZeroU16;

#[derive(Debug, Clone)]
/// **Exponential moving average** (EMA) over scalar samples.
///
/// Stores a single `f64` state and updates it in O(1) time with O(1) memory. This is also called
/// an exponentially weighted moving average; older samples are never dropped, but their influence
/// on the average decays exponentially.
///
/// With a new sample `x`, the EMA is updated as: `ema = alpha * x + (1.0 - alpha) * ema_prev`
///
/// `alpha` ∈ (0, 1) is the weight of the newest sample, whereas `1 - alpha` is the weight of the
/// previous EMA value. `alpha` is called the *smoothing factor*: larger `alpha` reacts faster to
/// sample changes, while smaller `alpha` smooths the average.
///
/// You can construct this type from a half-life integer by using [`Ema::from_half_life`]. After
/// `half_life` updates, a sample contributes half as much as when it was added. With half-life
/// `H`, the most recent `N` samples contribute `1 - 2^(-N/H)` of the total weight (once warmed
/// up). Example: if `H = 50`, newest 50 samples = 50%, 100 = 75%, 150 = 87.5%, etc.
pub struct Ema {
    /// Smoothing factor: weight of the newest sample.
    alpha: f64,

    /// Current EMA value, if any samples have been added.
    value: Option<f64>,
}

impl Ema {
    /// EMA preset: half-life 50 samples (good default for peer message latency).
    pub fn with_half_life_50() -> Self {
        Self::from_half_life(NonZeroU16::new(50).expect("non-zero"))
    }

    /// EMA preset: half-life 1000 samples (low noise, good for e.g., block validation time).
    pub fn with_half_life_1000() -> Self {
        Self::from_half_life(NonZeroU16::new(1_000).expect("non-zero"))
    }

    /// Constructs an EMA from a half-life measured in samples.
    ///
    /// After `half_life` updates, a sample's weight is halved. This chooses:
    /// `alpha = 1 - 2^(-1/half_life)`.
    pub fn from_half_life(half_life: NonZeroU16) -> Self {
        let hl = half_life.get() as f64;

        // alpha is guaranteed in (0, 1) here for any finite hl > 0
        let alpha = 1.0 - 2f64.powf(-1.0 / hl);
        Self::new(alpha)
    }

    /// Constructs an EMA from `alpha` in (0, 1). Panics if `alpha` is outside this range.
    fn new(alpha: f64) -> Self {
        assert!(alpha > 0.0 && alpha < 1.0, "invalid alpha: {alpha}");
        Self { alpha, value: None }
    }

    /// Adds a sample to the EMA.
    pub fn add(&mut self, x: f64) {
        self.value = Some(match self.value {
            None => x, // first sample
            Some(prev) => self.alpha * x + (1.0 - self.alpha) * prev,
        });
    }

    /// Returns the current EMA value, or `None` if no samples have been added yet.
    pub fn value(&self) -> Option<f64> {
        self.value
    }

    /// Returns the `alpha` smoothing factor used by this EMA.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn test_ema_alpha_zero() {
        let _ = Ema::new(0.0);
    }

    #[test]
    #[should_panic]
    fn test_ema_alpha_one() {
        let _ = Ema::new(1.0);
    }

    #[test]
    fn test_ema_alpha_half() {
        // EMA update: v = 0.5*x + 0.5*v
        let mut avg = Ema::new(0.5);
        assert_eq!(avg.value(), None);

        // An alpha of 0.5 means that half-life is just 1 sample
        assert_eq!(
            avg.alpha(),
            Ema::from_half_life(NonZeroU16::new(1).unwrap()).alpha()
        );

        let inputs = [10.0, 20.0, 30.0, 40.0, 50.0, 1.0, 99.0, 20.0, 0.0];
        let expected = [
            10.0, 15.0, 22.5, 31.25, 40.625, 20.8125, 59.90625, 39.953125, 19.9765625,
        ];

        for (x, &e) in inputs.into_iter().zip(expected.iter()) {
            avg.add(x);
            assert_eq!(avg.value(), Some(e));
        }
    }

    fn assert_close(got: f64, expected: f64) {
        let tol = 1e-12;
        let diff = (got - expected).abs();
        assert!(diff <= tol, "got {got:.15}, expected {expected:.15}");
    }

    #[test]
    fn test_ema_from_half_life() {
        fn ema_from_u16(half_life: u16) -> Ema {
            let hl = NonZeroU16::new(half_life).expect("half_life should be positive");
            Ema::from_half_life(hl)
        }

        // Precomputed reference values: alpha = 1 - 2^(-1/half_life)
        let alpha_hl_50 = 0.013_767_295_506_640_798;
        let alpha_hl_1000 = 0.000_692_907_009_547_494_3;

        assert_close(ema_from_u16(1).alpha(), 0.5);
        assert_close(ema_from_u16(2).alpha(), 0.292_893_218_813_452_4);
        assert_close(ema_from_u16(3).alpha(), 0.206_299_474_015_900_2);
        assert_close(ema_from_u16(4).alpha(), 0.159_103_584_746_285_5);
        assert_close(ema_from_u16(5).alpha(), 0.129_449_436_703_875_88);
        assert_close(ema_from_u16(6).alpha(), 0.109_101_281_859_660_73);
        assert_close(ema_from_u16(7).alpha(), 0.094_276_335_736_093_29);
        assert_close(ema_from_u16(8).alpha(), 0.082_995_956_795_328_78);
        assert_close(ema_from_u16(9).alpha(), 0.074_125_287_712_709_54);
        assert_close(ema_from_u16(10).alpha(), 0.066_967_008_463_192_59);
        assert_close(ema_from_u16(20).alpha(), 0.034_063_671_075_154_4);
        assert_close(ema_from_u16(40).alpha(), 0.017_179_401_454_748_944);
        assert_close(ema_from_u16(50).alpha(), alpha_hl_50);
        assert_close(Ema::with_half_life_50().alpha(), alpha_hl_50);
        assert_close(ema_from_u16(100).alpha(), 0.006_907_504_562_964_073);
        assert_close(ema_from_u16(200).alpha(), 0.003_459_737_172_132_216);
        assert_close(ema_from_u16(1000).alpha(), alpha_hl_1000);
        assert_close(Ema::with_half_life_1000().alpha(), alpha_hl_1000);
        assert_close(ema_from_u16(u16::MAX).alpha(), 0.000_010_576_692_072_161_72);

        for i in 1..=u16::MAX {
            // All values produce a valid alpha value (or else this would panic).
            let _ = ema_from_u16(i);
        }
    }

    #[test]
    fn test_ema_half_life_50() {
        let mut ema = Ema::from_half_life(NonZeroU16::new(50).unwrap());
        assert_eq!(ema.alpha(), Ema::with_half_life_50().alpha());
        assert_eq!(ema.value(), None);

        let baseline = 120.0; // ms
        let up = 120.2; // +0.2 ms
        let down = 119.8; // -0.2 ms

        // 1) Stabilize at baseline with 100 identical samples.
        for _ in 0..100 {
            ema.add(baseline);
        }
        assert_close(ema.value().unwrap(), 120.0);

        // 2) Step up for 100 samples.
        // With half-life=50:
        // - after 50 samples, you're ~50% to the new level: 120.1
        // - after 100 samples, you're ~75% to the new level: 120.15
        let up_expected = [
            (1, 120.002_753_459_101_33),
            (2, 120.005_469_010_517_54),
            (5, 120.013_393_401_692_62),
            (20, 120.048_428_343_348_93),
            (50, 120.099_999_999_999_94),
            (100, 120.149_999_999_999_9),
        ];

        for i in 1..=100 {
            ema.add(up);
            if let Some((_, e)) = up_expected.iter().find(|(k, _)| *k == i) {
                assert_close(ema.value().unwrap(), *e);
            }
        }

        // 3) Step down for 100 samples.
        // Starting near 120.15, stepping to 119.8 (delta -0.35):
        // - after 50 samples: 120.15 - 0.175 = 119.975
        // - after 100 samples: 120.15 - 0.2625 = 119.8875
        let down_expected = [
            (1, 120.145_181_446_572_58),
            (2, 120.140_429_231_594_2),
            (5, 120.126_561_547_037_78),
            (20, 120.065_250_399_139_22),
            (50, 119.974_999_999_999_9),
            (100, 119.887_499_999_999_92),
        ];

        for i in 1..=100 {
            ema.add(down);
            if let Some((_, e)) = down_expected.iter().find(|(k, _)| *k == i) {
                assert_close(ema.value().unwrap(), *e);
            }
        }
    }
}
