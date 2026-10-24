//! Mic capture and speaker playback (plumbing, not a capability).

mod mic;
mod player;

pub use mic::Recording;
pub use player::{AudioQueue, Player};

/// Resample mono audio. Box-averages when downsampling (cheap anti-aliasing),
/// interpolates linearly when upsampling. Adequate for speech.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = (input.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * ratio;
            let j = p as usize;
            if ratio > 1.0 {
                let end = (((i + 1) as f64 * ratio) as usize).clamp(j + 1, input.len());
                input[j..end].iter().sum::<f32>() / (end - j) as f32
            } else {
                let (a, b) = (input[j], *input.get(j + 1).unwrap_or(&input[j]));
                a + (b - a) * (p - j as f64) as f32
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn resample_lengths() {
        let x = vec![0.5; 48_000];
        assert_eq!(super::resample(&x, 48_000, 16_000).len(), 16_000);
        assert_eq!(super::resample(&x[..24_000], 24_000, 48_000).len(), 48_000);
        assert!(super::resample(&x, 48_000, 16_000).iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }
}
