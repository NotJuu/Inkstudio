/// Pressure response curve: piecewise-linear through control points on [0,1]×[0,1],
/// pre-sampled into a lookup table so evaluation in the dab loop is one index.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    points: Vec<(f32, f32)>,
    lut: Vec<f32>,
}

const LUT: usize = 256;

impl Curve {
    pub fn new(mut points: Vec<(f32, f32)>) -> Self {
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        if points.first().is_none_or(|p| p.0 > 0.0) {
            points.insert(0, (0.0, points.first().map_or(0.0, |p| p.1)));
        }
        if points.last().is_none_or(|p| p.0 < 1.0) {
            points.push((1.0, points.last().map_or(1.0, |p| p.1)));
        }
        let lut = (0..LUT)
            .map(|i| {
                let x = i as f32 / (LUT - 1) as f32;
                let j = points
                    .windows(2)
                    .position(|w| x <= w[1].0)
                    .unwrap_or(points.len() - 2);
                let (a, b) = (points[j], points[j + 1]);
                let t = if b.0 > a.0 {
                    (x - a.0) / (b.0 - a.0)
                } else {
                    0.0
                };
                (a.1 + (b.1 - a.1) * t).clamp(0.0, 1.0)
            })
            .collect();
        Self { points, lut }
    }

    pub fn linear() -> Self {
        Self::new(vec![(0.0, 0.0), (1.0, 1.0)])
    }

    /// y = x^g sampled at 9 points. g > 1: needs more pressure; g < 1: responds early.
    pub fn gamma(g: f32) -> Self {
        Self::new(
            (0..=8)
                .map(|i| i as f32 / 8.0)
                .map(|x| (x, x.powf(g)))
                .collect(),
        )
    }

    pub fn points(&self) -> &[(f32, f32)] {
        &self.points
    }

    #[inline]
    pub fn eval(&self, x: f32) -> f32 {
        let f = x.clamp(0.0, 1.0) * (LUT - 1) as f32;
        let i = f as usize;
        if i >= LUT - 1 {
            return self.lut[LUT - 1];
        }
        let t = f - i as f32;
        self.lut[i] + (self.lut[i + 1] - self.lut[i]) * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_and_gamma() {
        let l = Curve::linear();
        assert!((l.eval(0.3) - 0.3).abs() < 1e-3);
        let g = Curve::gamma(2.0);
        assert!((g.eval(0.5) - 0.25).abs() < 0.02);
        assert_eq!(g.eval(-1.0), 0.0);
        assert_eq!(g.eval(2.0), 1.0);
    }
}
