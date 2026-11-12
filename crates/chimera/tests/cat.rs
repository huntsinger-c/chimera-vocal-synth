//! The house cat, measured from its sound: what makes each call that call, that the keys pitch
//! it, that every call ends in silence, that no setting breaks it, and that the output does not
//! depend on the host's block size. (The creatures' golden fixtures record the TypeScript engine,
//! which had no cat.)
use chimera::analysis::fft::{Fft, hann, power_spectrum_db};
use chimera::analysis::pitch::{YinOpts, yin_pitch};
use chimera::creatures::{CreatureDef, CreatureVoice, creature_by_id};

const FS: f64 = 48000.0;

fn cat() -> &'static CreatureDef {
    creature_by_id("cat").expect("the cat is in the registry")
}

fn render(v: &mut CreatureVoice, seconds: f64) -> Vec<f32> {
    let mut out = vec![0.0f32; (seconds * FS) as usize];
    for chunk in out.chunks_mut(128) {
        v.render(chunk);
    }
    out
}

/// A call from a fresh cat, to 0.5 s past its last key.
fn call(id: &str, pitch: Option<f64>) -> Vec<f32> {
    let def = cat();
    let c = &def.calls[def.call_index(id).unwrap()];
    let mut v = CreatureVoice::new(def, FS, &[]);
    v.play_call_id(id, pitch, 1.0);
    let out = render(&mut v, c.keys.last().unwrap().t + 0.5);
    assert_eq!(v.active_call(), None, "{id} is over half a second after its last key");
    out
}

fn span(x: &[f32], t0: f64, t1: f64) -> &[f32] {
    &x[(t0 * FS) as usize..((t1 * FS) as usize).min(x.len())]
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn peak(x: &[f32]) -> f64 {
    x.iter().fold(0.0f64, |m, v| m.max(v.abs() as f64))
}

fn frame(x: &[f32], t: f64, n: usize) -> Vec<f64> {
    let mid = (t * FS) as usize;
    let start = mid.saturating_sub(n / 2).min(x.len() - n);
    x[start..start + n].iter().map(|&v| v as f64).collect()
}

/// YIN over 43 ms around `t`: the pitch (0 when there is none) and the aperiodicity.
fn pitch_at(x: &[f32], t: f64) -> (f64, f64) {
    let p = yin_pitch(&frame(x, t, 2048), FS, YinOpts { min_f0: Some(60.0), max_f0: Some(2000.0), ..Default::default() });
    (p.f0, p.aperiodicity)
}

/// Where the sound's power sits (Hz), over 85 ms around `t`.
fn centroid_at(x: &[f32], t: f64) -> f64 {
    let n = 4096;
    let db = power_spectrum_db(&frame(x, t, n), &Fft::new(n), &hann(n));
    let (mut num, mut den) = (0.0, 0.0);
    for (k, d) in db.iter().enumerate().skip(4) {
        let p = 10f64.powf(d / 10.0);
        num += p * k as f64 * FS / n as f64;
        den += p;
    }
    num / den
}

/// How many times a second the loudness pulses between `t0` and `t1` (the strongest repeat of the
/// 1 ms loudness envelope between 15 and 50 Hz), and how deep the pulsing is (0 steady, 1 to silence).
fn pulsing(x: &[f32], t0: f64, t1: f64) -> (f64, f64) {
    let hop = (FS / 1000.0) as usize;
    let env: Vec<f64> = span(x, t0, t1).chunks(hop).map(rms).collect();
    let mean = env.iter().sum::<f64>() / env.len() as f64;
    let e: Vec<f64> = env.iter().map(|v| v - mean).collect();
    let corr = |lag: usize| (0..e.len() - lag).map(|i| e[i] * e[i + lag]).sum::<f64>() / (e.len() - lag) as f64;
    let (mut best, mut best_lag) = (f64::MIN, 0);
    for lag in 20..=66 {
        let c = corr(lag);
        if c > best {
            (best, best_lag) = (c, lag);
        }
    }
    // refine the lag with a parabola through its neighbours
    let (a, b, c) = (corr(best_lag - 1), best, corr(best_lag + 1));
    let off = if (a - 2.0 * b + c).abs() > 1e-30 { 0.5 * (a - c) / (a - 2.0 * b + c) } else { 0.0 };
    let (lo, hi) = env.iter().fold((f64::MAX, 0.0f64), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    (1000.0 / (best_lag as f64 + off), (hi - lo) / (hi + lo))
}

#[test]
fn a_meow_rises_and_falls_swells_and_brightens_as_the_mouth_opens() {
    let m = call("meow", None);
    // the pitch: up from 580 Hz to 714 as the mouth opens, down to 458 as it shuts
    let (start, top, end) = (pitch_at(&m, 0.1).0, pitch_at(&m, 0.33).0, pitch_at(&m, 0.9).0);
    assert!(start > 540.0 && start < 620.0, "pitch at 0.1 s {start:.0} Hz");
    assert!(top > 690.0 && top < 735.0, "pitch at 0.33 s {top:.0} Hz");
    assert!(end > 430.0 && end < 490.0, "pitch at 0.9 s {end:.0} Hz");
    // the wah: the sound's weight goes up as the mouth opens and comes back down as it shuts
    let (c0, c1, c2) = (centroid_at(&m, 0.1), centroid_at(&m, 0.2), centroid_at(&m, 0.9));
    assert!(c1 > 1.35 * c0 && c1 > 1.6 * c2, "centroid {c0:.0} → {c1:.0} → {c2:.0} Hz");
    // and it swells: over three times as loud with the mouth open as at either end
    let (l0, l1, l2) = (rms(span(&m, 0.07, 0.13)), rms(span(&m, 0.35, 0.6)), rms(span(&m, 0.87, 0.93)));
    assert!(l1 > 3.0 * l0 && l1 > 3.0 * l2, "level {l0:.3} → {l1:.3} → {l2:.3}");
    // with room under the limiter's knee (0.7)
    assert!(peak(&m) > 0.25 && peak(&m) < 0.6, "peak {:.3}", peak(&m));
}

#[test]
fn the_strongest_part_of_a_meow_is_its_second_harmonic_between_1_and_2_khz() {
    // Shipley et al. 1991: the strongest frequency of a call is nearly a harmonic of its pitch,
    // between 1 and 2 kHz, carrying 12-38 times the energy of the fundamental on average
    let m = call("meow", None);
    let n = 4096;
    let db = power_spectrum_db(&frame(&m, 0.45, n), &Fft::new(n), &hann(n));
    let bin = FS / n as f64;
    let strongest = (4..n / 2).max_by(|a, b| db[*a].total_cmp(&db[*b])).unwrap() as f64 * bin;
    let f0 = pitch_at(&m, 0.45).0;
    assert!(strongest > 1000.0 && strongest < 2000.0, "strongest at {strongest:.0} Hz");
    assert!((strongest / f0 - 2.0).abs() < 0.06, "strongest {strongest:.0} Hz over a pitch of {f0:.0} Hz");
    let at = |f: f64| ((f / bin).round() as usize - 2..=(f / bin).round() as usize + 2).map(|k| db[k]).fold(f64::MIN, f64::max);
    let ratio = 10f64.powf((at(strongest) - at(f0)) / 10.0);
    // (a top of 650 Hz put the harmonic on the open mouth's resonance: 165 times, and the pitch read an octave up)
    assert!(ratio > 5.0 && ratio < 60.0, "the strongest part has {ratio:.1} times the fundamental's energy");
}

#[test]
fn the_keys_pitch_a_meow() {
    // the call's pitches scale with the key against the call's reference (its top, 720 Hz)
    let m = call("meow", Some(360.0));
    let (top, end) = (pitch_at(&m, 0.33).0, pitch_at(&m, 0.9).0);
    assert!((top / 357.0 - 1.0).abs() < 0.03, "pitch at the top {top:.1} Hz");
    assert!((end / 229.0 - 1.0).abs() < 0.05, "pitch at the end {end:.1} Hz");
}

#[test]
fn a_trill_rises_and_is_rolled_at_the_purrs_rate() {
    let t = call("trill", None);
    let (start, end) = (pitch_at(&t, 0.06).0, pitch_at(&t, 0.5).0);
    assert!(start > 290.0 && start < 360.0 && end > 560.0 && end < 620.0, "pitch {start:.0} → {end:.0} Hz");
    // the roll: the loudness pulses 30 times a second, most of the way down to silence
    let (rate, depth) = pulsing(&t, 0.03, 0.33);
    assert!(rate > 28.5 && rate < 31.5 && depth > 0.5, "rolled at {rate:.1} Hz, depth {depth:.2}");
}

#[test]
fn a_purr_pulses_at_its_rate_through_the_breath_in_and_the_breath_out() {
    let p = call("purr", None);
    for breath in 0..4 {
        // the breath starts 0.13 s into the call and takes 1.24 s: 0.66 s in, 0.58 s out
        let t0 = 0.13 + 1.24 * breath as f64;
        let ((rate_in, depth_in), (rate_out, depth_out)) = (pulsing(&p, t0 + 0.15, t0 + 0.55), pulsing(&p, t0 + 0.76, t0 + 1.16));
        // 25 pulses a second, 2% slower on the breath in and 2% faster on the breath out
        assert!(rate_in > 24.1 && rate_in < 24.9, "breath {breath} in: {rate_in:.2} pulses a second");
        assert!(rate_out > 25.1 && rate_out < 25.9, "breath {breath} out: {rate_out:.2} pulses a second");
        // pulses with silence between them, not a tone
        assert!(depth_in > 0.9 && depth_out > 0.9, "breath {breath}: pulsing depth {depth_in:.2} in, {depth_out:.2} out");
        let (loud_in, loud_out, turn) = (rms(span(&p, t0 + 0.15, t0 + 0.55)), rms(span(&p, t0 + 0.76, t0 + 1.16)), rms(span(&p, t0 + 0.64, t0 + 0.675)));
        // both ways sound, about alike (the first breath in is still fading up: the mouth is shutting)
        if breath > 0 {
            assert!(loud_in > 0.5 * loud_out && loud_in < 2.0 * loud_out, "breath {breath}: RMS {loud_in:.4} in, {loud_out:.4} out");
        }
        // and it stops for a moment where the breath turns
        assert!(turn < 0.5 * loud_out, "breath {breath}: RMS {turn:.4} at the turn, {loud_out:.4} out");
    }
    // close enough to hear: a purr is 30 dB under a meow at the same distance
    assert!(peak(&p) > 0.05 && peak(&p) < 0.3, "peak {:.3}", peak(&p));
}

#[test]
fn a_hiss_is_noise_with_no_pitch() {
    let h = call("hiss", None);
    let (f0, aperiodicity) = pitch_at(&h, 0.4);
    assert!(f0 == 0.0 && aperiodicity > 0.5, "pitch {f0:.0} Hz, aperiodicity {aperiodicity:.2}");
    // (injected one section past the slit, not a centimetre, its weight sat at 15 kHz: a sizzle)
    let c = centroid_at(&h, 0.4);
    assert!(c > 4000.0 && c < 11000.0, "centroid {c:.0} Hz");
    let level = rms(span(&h, 0.2, 0.6));
    assert!(level > 0.02 && level < 0.1, "RMS {level:.4}");
}

#[test]
fn every_call_ends_in_silence() {
    for c in &cat().calls {
        let x = call(c.id, None);
        assert!(x.iter().all(|v| v.is_finite()), "{}: not finite", c.id);
        let tail = rms(&x[x.len() - (0.15 * FS) as usize..]);
        assert!(tail < 1e-4, "{}: RMS {tail:.2e} half a second after its last key", c.id);
    }
}

#[test]
fn no_setting_breaks_it() {
    // every corner of the five settings that interact, with the other three at their extremes
    // too: the airway stays finite, sounds, and never sits on the limiter
    let def = cat();
    let idx = |k: &str| def.param_index(k).unwrap();
    let corners: Vec<Vec<(usize, f64)>> = (0..32u32)
        .map(|bits| {
            let pick = |bit: u32, k: &str| {
                let p = &def.params[idx(k)];
                (idx(k), if bits >> bit & 1 == 1 { p.max } else { p.min })
            };
            let mut v = vec![pick(0, "pitch"), pick(1, "mouth"), pick(2, "purr"), pick(3, "pressure"), pick(4, "hiss")];
            v.push((idx("rough"), if bits % 3 == 0 { 1.0 } else { 0.0 }));
            v.push((idx("tongue"), if bits % 2 == 0 { 1.0 } else { 0.0 }));
            v.push((idx("purrRate"), if bits % 5 == 0 { 45.0 } else { 15.0 }));
            v
        })
        .collect();
    let worst: Vec<(f64, f64)> = std::thread::scope(|s| {
        let hs: Vec<_> = corners
            .iter()
            .map(|c| {
                s.spawn(move || {
                    let mut v = CreatureVoice::new(def, FS, c);
                    v.set_gate(1.0);
                    let x = render(&mut v, 1.0);
                    assert!(x.iter().all(|y| y.is_finite()), "{c:?}: not finite");
                    (rms(span(&x, 0.5, 1.0)), peak(&x))
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for (c, (level, pk)) in corners.iter().zip(&worst) {
        assert!(*level < 0.35 && *pk < 0.95, "{c:?}: RMS {level:.3}, peak {pk:.3}");
    }
    // the loudest corner is a top note at full breath through a wide mouth
    assert!(worst.iter().any(|(level, _)| *level > 0.1), "no corner sounds: {worst:?}");
}

#[test]
fn the_output_does_not_depend_on_the_block_size() {
    let def = cat();
    let run = |block: usize| {
        let mut v = CreatureVoice::new(def, FS, &[]);
        let n = (2.6 * FS) as usize;
        let mut out = vec![0.0f32; n];
        // a meow, the mouth moved by hand under it, a purr started over its tail, a held note
        type Op<'a> = (usize, &'a dyn Fn(&mut CreatureVoice));
        let ops: [Op; 5] = [
            (0, &|v| v.play_call_id("meow", Some(500.0), 1.0)),
            (20000, &|v| v.set_params(&[(def.param_index("tongue").unwrap(), 0.9), (def.param_index("rough").unwrap(), 0.4)])),
            (50000, &|v| v.play_call_id("purr", None, 1.0)),
            (100000, &|v| v.stop_call()),
            (101000, &|v| v.set_gate(1.0)),
        ];
        let (mut i, mut oi) = (0, 0);
        while i < n {
            while oi < ops.len() && ops[oi].0 <= i {
                ops[oi].1(&mut v);
                oi += 1;
            }
            let end = n.min(i + block).min(ops.get(oi).map_or(usize::MAX, |o| o.0));
            v.render(&mut out[i..end]);
            i = end;
        }
        out
    };
    let reference = run(128);
    assert!(rms(span(&reference, 2.3, 2.6)) > 0.02, "the held note sounds");
    for block in [1, 77, 4096] {
        let got = run(block);
        let first = reference.iter().zip(&got).position(|(a, b)| a.to_bits() != b.to_bits());
        assert_eq!(first, None, "block size {block}: differs from 128-sample blocks at sample {first:?}");
    }
}
