//! Nelder-Mead simplex optimizer — gradient-free parameter tuning.
//!
//! Pure Rust implementation for optimizing SNN weights without external
//! dependencies. Nelder-Mead is ideal here: the objective function
//! (Pearson correlation) is noisy, non-differentiable, and low-dimensional.

/// Minimize `f(x)` using the Nelder-Mead simplex method.
///
/// Returns `(best_params, best_value)`.
///
/// # Arguments
/// * `f` — objective function to minimize
/// * `initial` — starting parameter vector (n dimensions)
/// * `max_iterations` — upper bound on iterations
/// * `tolerance` — convergence threshold (stop when simplex range < tolerance)
pub fn nelder_mead<F>(
    f: &F,
    initial: &[f64],
    max_iterations: usize,
    tolerance: f64,
) -> (Vec<f64>, f64)
where
    F: Fn(&[f64]) -> f64,
{
    let n = initial.len();
    let alpha = 1.0; // reflection
    let gamma = 2.0; // expansion
    let rho = 0.5; // contraction
    let sigma = 0.5; // shrink

    // Initialize simplex: n+1 vertices
    let mut simplex: Vec<Vec<f64>> = Vec::with_capacity(n + 1);
    simplex.push(initial.to_vec());
    for i in 0..n {
        let mut vertex = initial.to_vec();
        vertex[i] += if vertex[i].abs() > 1e-10 {
            vertex[i] * 0.05 // 5% perturbation
        } else {
            0.05
        };
        simplex.push(vertex);
    }

    let mut values: Vec<f64> = simplex.iter().map(|v| f(v)).collect();

    for _iter in 0..max_iterations {
        // Sort by function value
        let mut indices: Vec<usize> = (0..=n).collect();
        indices.sort_by(|&a, &b| values[a].partial_cmp(&values[b]).unwrap());

        let sorted_simplex: Vec<Vec<f64>> = indices.iter().map(|&i| simplex[i].clone()).collect();
        let sorted_values: Vec<f64> = indices.iter().map(|&i| values[i]).collect();
        simplex = sorted_simplex;
        values = sorted_values;

        // Check convergence
        let range = values[n] - values[0];
        if range < tolerance {
            break;
        }

        // Centroid of all points except worst
        let mut centroid = vec![0.0; n];
        for vertex in simplex.iter().take(n) {
            for (j, c) in centroid.iter_mut().enumerate() {
                *c += vertex[j];
            }
        }
        for c in centroid.iter_mut() {
            *c /= n as f64;
        }

        // Reflection
        let reflected: Vec<f64> = (0..n)
            .map(|j| centroid[j] + alpha * (centroid[j] - simplex[n][j]))
            .collect();
        let fr = f(&reflected);

        if fr < values[0] {
            // Try expansion
            let expanded: Vec<f64> = (0..n)
                .map(|j| centroid[j] + gamma * (reflected[j] - centroid[j]))
                .collect();
            let fe = f(&expanded);
            if fe < fr {
                simplex[n] = expanded;
                values[n] = fe;
            } else {
                simplex[n] = reflected;
                values[n] = fr;
            }
        } else if fr < values[n - 1] {
            simplex[n] = reflected;
            values[n] = fr;
        } else {
            // Contraction
            let contracted: Vec<f64> = (0..n)
                .map(|j| centroid[j] + rho * (simplex[n][j] - centroid[j]))
                .collect();
            let fc = f(&contracted);
            if fc < values[n] {
                simplex[n] = contracted;
                values[n] = fc;
            } else {
                // Shrink
                let best_vertex = simplex[0].clone();
                for i in 1..=n {
                    for j in 0..n {
                        simplex[i][j] = best_vertex[j] + sigma * (simplex[i][j] - best_vertex[j]);
                    }
                    values[i] = f(&simplex[i]);
                }
            }
        }
    }

    // Return best
    let best_idx = values
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0);
    (simplex[best_idx].clone(), values[best_idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimize_quadratic() {
        // Minimize f(x,y) = (x-3)^2 + (y-5)^2
        // Global minimum at (3, 5) with value 0
        let f = |x: &[f64]| (x[0] - 3.0).powi(2) + (x[1] - 5.0).powi(2);
        let (best, val) = nelder_mead(&f, &[0.0, 0.0], 1000, 1e-10);
        assert!(
            (best[0] - 3.0).abs() < 0.01,
            "x should be ~3.0, got {}",
            best[0]
        );
        assert!(
            (best[1] - 5.0).abs() < 0.01,
            "y should be ~5.0, got {}",
            best[1]
        );
        assert!(val < 0.001, "minimum value should be ~0, got {}", val);
    }

    #[test]
    fn test_minimize_rosenbrock() {
        // Rosenbrock: f(x,y) = (1-x)^2 + 100*(y-x^2)^2
        // Minimum at (1, 1)
        let f = |x: &[f64]| (1.0 - x[0]).powi(2) + 100.0 * (x[1] - x[0].powi(2)).powi(2);
        let (best, _val) = nelder_mead(&f, &[0.0, 0.0], 5000, 1e-12);
        assert!(
            (best[0] - 1.0).abs() < 0.05,
            "x should be ~1.0, got {}",
            best[0]
        );
        assert!(
            (best[1] - 1.0).abs() < 0.05,
            "y should be ~1.0, got {}",
            best[1]
        );
    }

    #[test]
    fn test_minimize_higher_dim() {
        // f(x) = sum((xi - i)^2) for i in 0..5
        let f = |x: &[f64]| -> f64 {
            x.iter()
                .enumerate()
                .map(|(i, &xi)| (xi - i as f64).powi(2))
                .sum()
        };
        let (best, val) = nelder_mead(&f, &[10.0; 5], 5000, 1e-10);
        for (i, &b) in best.iter().enumerate() {
            assert!(
                (b - i as f64).abs() < 0.1,
                "x[{}] should be ~{}, got {}",
                i,
                i,
                b
            );
        }
        assert!(val < 0.01, "minimum should be ~0, got {}", val);
    }

    #[test]
    fn test_convergence_with_tolerance() {
        // 2D quadratic — verify early stopping via tolerance
        let f = |x: &[f64]| x[0].powi(2) + x[1].powi(2);
        let (best, val) = nelder_mead(&f, &[2.0, 3.0], 2000, 1e-10);
        assert!(best[0].abs() < 0.1, "x should be ~0: {}", best[0]);
        assert!(best[1].abs() < 0.1, "y should be ~0: {}", best[1]);
        assert!(val < 0.01, "minimum should be ~0: {}", val);
    }
}
