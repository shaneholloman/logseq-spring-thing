pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let mut sum = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        sum += (*x as f64) * (*y as f64);
    }
    sum as f32
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;

    for (x, y) in a.iter().zip(b.iter()) {
        let xf = *x as f64;
        let yf = *y as f64;
        dot += xf * yf;
        norm_a += xf * xf;
        norm_b += yf * yf;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-12 {
        return 0.0;
    }

    (dot / denom) as f32
}

pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let mut sum = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let d = (*x as f64) - (*y as f64);
        sum += d * d;
    }
    sum.sqrt() as f32
}

pub fn normalize_vector(v: &mut [f32]) {
    let norm = v
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();
    if norm < 1e-12 {
        return;
    }
    let inv = 1.0 / norm;
    for x in v.iter_mut() {
        *x = (*x as f64 * inv) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_product_basic() {
        assert!((dot_product(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]) - 32.0).abs() < 1e-6);
    }

    #[test]
    fn dot_product_zero_vector() {
        assert_eq!(dot_product(&[0.0, 0.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn dot_product_mismatched_lengths() {
        assert_eq!(dot_product(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn dot_product_empty() {
        let e: &[f32] = &[];
        assert_eq!(dot_product(e, e), 0.0);
    }

    #[test]
    fn cosine_identical() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_unit_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_scaled() {
        let a = vec![3.0, 4.0];
        let b = vec![6.0, 8.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_zero_vector() {
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn cosine_both_zero() {
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[0.0, 0.0]), 0.0);
    }

    #[test]
    fn cosine_empty() {
        let e: &[f32] = &[];
        assert_eq!(cosine_similarity(e, e), 0.0);
    }

    #[test]
    fn cosine_mismatched_lengths() {
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0, 2.0, 3.0]), 0.0);
    }

    #[test]
    fn cosine_very_small_values() {
        let a = vec![1e-20f32, 1e-20, 1e-20];
        let b = vec![1e-20f32, 1e-20, 1e-20];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn cosine_very_large_values() {
        let a = vec![1e30f32, 0.0, 0.0];
        let b = vec![1e30f32, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn cosine_single_element() {
        assert!((cosine_similarity(&[5.0], &[3.0]) - 1.0).abs() < 1e-6);
        assert!((cosine_similarity(&[5.0], &[-3.0]) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_nan_input() {
        let a = vec![f32::NAN, 1.0];
        let b = vec![1.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.is_nan() || sim.abs() <= 1.0);
    }

    #[test]
    fn cosine_45_degree() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 1.0];
        let expected = 1.0f32 / 2.0f32.sqrt();
        assert!((cosine_similarity(&a, &b) - expected).abs() < 1e-5);
    }

    #[test]
    fn euclidean_basic() {
        assert!((euclidean_distance(&[0.0, 0.0], &[3.0, 4.0]) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn euclidean_identical() {
        assert_eq!(euclidean_distance(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]), 0.0);
    }

    #[test]
    fn euclidean_zero_vectors() {
        assert_eq!(euclidean_distance(&[0.0, 0.0], &[0.0, 0.0]), 0.0);
    }

    #[test]
    fn euclidean_mismatched_lengths() {
        assert_eq!(euclidean_distance(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn euclidean_1d() {
        assert!((euclidean_distance(&[3.0], &[7.0]) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn normalize_basic() {
        let mut v = vec![3.0, 4.0];
        normalize_vector(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn normalize_already_unit() {
        let mut v = vec![1.0, 0.0, 0.0];
        normalize_vector(&mut v);
        assert!((v[0] - 1.0).abs() < 1e-6);
        assert!(v[1].abs() < 1e-6);
        assert!(v[2].abs() < 1e-6);
    }

    #[test]
    fn normalize_zero_vector() {
        let mut v = vec![0.0, 0.0, 0.0];
        normalize_vector(&mut v);
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn normalize_empty() {
        let mut v: Vec<f32> = vec![];
        normalize_vector(&mut v);
        assert!(v.is_empty());
    }

    #[test]
    fn normalize_single_negative() {
        let mut v = vec![-5.0];
        normalize_vector(&mut v);
        assert!((v[0] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn normalize_high_dim() {
        let mut v = vec![1.0f32; 384];
        normalize_vector(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }
}
