use criterion::{criterion_group, criterion_main, Criterion};
use rayon::prelude::*;
use std::{hint::black_box, time::Duration};

#[path = "../bin/cmd/wallet/mod.rs"]
#[allow(unused)]
mod wallet;
use wallet::vanity::*;

/// Benches `cast wallet vanity`
///
/// Left or right matchers, with or without nonce do not change the outcome.
///
/// Regex matchers get optimised away even with a black_box.
fn vanity(c: &mut Criterion) {
    let mut g = c.benchmark_group("vanity");

    g.sample_size(500);
    g.noise_threshold(0.04);
    g.measurement_time(Duration::from_secs(30));

    let generator = PrivateKeyGenerator {};

    g.bench_function("private key generator", |b| b.iter(|| black_box(generator.generate())));

    let generator = MnemonicGenerator { words: 12 };

    g.bench_function("mnemonic generator", |b| b.iter(|| black_box(generator.generate())));

    // 1

    g.sample_size(100);
    g.noise_threshold(0.02);

    let generator = PrivateKeyGenerator {};

    g.bench_function("match 1", |b| {
        let m = LeftHexMatcher { left: vec![0] };
        b.iter(|| wallet_generator(generator).find_any(|(_, addr)| black_box(m.is_match(&addr))))
    });

    // 2

    g.sample_size(10);
    g.noise_threshold(0.01);
    g.measurement_time(Duration::from_secs(60));

    let generator = PrivateKeyGenerator {};

    g.bench_function("match 2", |b| {
        let m = LeftHexMatcher { left: vec![0, 0] };
        b.iter(|| wallet_generator(generator).find_any(|(_, addr)| black_box(m.is_match(&addr))))
    });

    g.finish();
}

criterion_group!(vanity_benches, vanity);
criterion_main!(vanity_benches);
