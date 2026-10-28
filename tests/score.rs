use hypothesis_tournament::score::{self, EPSILON};

#[test]
fn a_certain_bet_is_never_quite_certain() {
    // One failure against a bet of 0% costs a lot, but not everything.
    let ll = score::log_likelihood(0.0, 1, 20);
    assert!(ll.is_finite());
    assert!((ll - (EPSILON.ln() + 19.0 * (1.0 - EPSILON).ln())).abs() < 1e-12);
}

#[test]
fn the_closer_bet_scores_higher() {
    // 5 of 20 failed: a 25% bet beats a 50% bet, which beats a 0% bet.
    let a = score::log_likelihood(0.25, 5, 20);
    let b = score::log_likelihood(0.5, 5, 20);
    let c = score::log_likelihood(0.0, 5, 20);
    assert!(a > b && b > c);
}

#[test]
fn shares_add_up_to_one() {
    let shares = score::shares(&[-1.0, -1.0, -1.0 - 100f64.ln()]);
    assert!((shares.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert!((shares[0] - shares[1]).abs() < 1e-12);
    assert!((shares[0] / shares[2] - 100.0).abs() < 1e-9);
}

#[test]
fn no_information_where_every_bet_agrees() {
    assert_eq!(score::information(&[0.5, 0.5], &[0.3, 0.3]), 0.0);
    // 0% against 100%: one run settles it (as far as the clamped bets allow).
    let bits = score::information(&[0.5, 0.5], &[0.0, 1.0]);
    assert!(bits > 0.85 && bits < 0.86, "{bits}");
    // An unlikely hypothesis counts for less.
    assert!(score::information(&[0.99, 0.01], &[0.0, 1.0]) < 0.1);
}
