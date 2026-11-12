//! House cat. A meow is the larynx singing 430–720 Hz under a mouth that opens and closes: over
//! a 9 cm tract the first resonance follows the jaw, the "wah" of mi-a-ow. A purr is the folds
//! parting about 25 times a second while the cat breathes in and then out through its nose, a
//! puff of air each time, whichever way the air is going. A hiss is an open glottis and a jet
//! over the tongue.
//!
//! What is measured and what is not:
//!  - The meow's pitch and its strongest part follow Shipley, Carterette & Buchwald (1991): calls
//!    at a mean 400–600 Hz (this one is at the top of that), the strongest frequency nearly a
//!    harmonic at 1–2 kHz, and the sound tied to how far the jaw is open.
//!  - The purr's rate and its breath follow Schötz & Eklund (2011), four cats: 21–27 pulses a
//!    second, the breath in about 650 ms and the breath out about 585 ms.
//!  - The purr's cause is argued: muscles twitching the folds apart (Remmers & Gautier 1972), or
//!    folds that flap that slowly by themselves (Herbst et al. 2023). The model takes no side: it
//!    sets how far the folds part and when, as the parametric glottis does for the voice.
//!  - The sizes are a 4 kg cat's, to the nearest half centimetre; no measurement is cited for them.

use super::build::{CMH2O, fill_areas};
use super::mammal::{MammalAirway, MammalSpec, mammal_airway};
use super::types::{CallDef, CallKey, CreatureDef, CreatureGroup, CreatureSim, Curve, GateTimes, ParamDef};
use crate::airway::network::{AirwayNetwork, NoiseSpec, Tube};
use crate::math::{PI, TAU, clamp, cos, exp, js_min, js_round, smoothstep, sqrt};

const PITCH: usize = 0;
const MOUTH: usize = 1;
const PURR: usize = 2;
const PRESSURE: usize = 3;
const TONGUE: usize = 4;
const PURR_RATE: usize = 5;
const ROUGH: usize = 6;
const HISS: usize = 7;

/// the airway, (length cm, area cm²): windpipe, throat, mouth (its area is the jaw's to set), nose
const TRACHEA: (f64, f64) = (11.0, 0.45);
const PHARYNX: (f64, f64) = (3.2, 0.9);
const ORAL: (f64, f64) = (5.5, 1.2);
const NASAL: (f64, f64) = (6.5, 0.3);
/// peak vibrating area of the folds (cm²) at the pitch a cat talks at: a 0.6 cm glottis against a
/// human's 1.5 cm
const FOLD_AREA: f64 = 0.045;
const TALKING_PITCH: f64 = 600.0;
/// how far the folds part for one pulse of a purr (cm²)
const PUFF_AREA: f64 = 0.03;
/// the part of a purr cycle the folds are apart for
const PUFF_DUTY: f64 = 0.3;
/// the glottis held open for a hiss (cm²), and how much of the gap under the palate the tongue
/// then fills: the slit the jet comes through
const HISS_AREA: f64 = 0.22;
const HISS_SLIT: f64 = 0.955;
/// one breath while purring (s), and the part of it spent breathing in: 652 ms in and 585 ms
/// out are the means of Schötz & Eklund's four cats
const BREATH: f64 = 1.24;
const BREATH_IN: f64 = 0.53;
/// how much faster the folds part on the breath out than their rate, and slower on the breath in
/// (24.3 pulses a second in and 24.8 out are those cats' means; one cat went the other way)
const BREATH_RATE: f64 = 0.02;
/// a purring cat's breath pressure against what it meows with
const PURR_PRESSURE: f64 = 0.3;
/// a meow is heard from across the room and a purr with an ear to the cat: a metre against
/// three centimetres, which is how much louder the purr is made
const PURR_CLOSE: f64 = 33.0;
/// the mouth wide open, at the teeth (cm²)
const GAPE: f64 = 3.0;
/// how loud the jet from the mouth's narrowest point is, and how far on it breaks up (cm)
const JET: f64 = 6e-5;
const JET_LENGTH: f64 = 1.0;

pub struct Cat {
    a: MammalAirway,
    tubes: Vec<(&'static str, Tube)>,
    velum: usize,
    /// where the folds are in a purr's cycle (0..1), how far a sample moves them on, and how far
    /// they part at its peak now (cm²)
    twitch_phase: f64,
    twitch_step: f64,
    twitch_area: f64,
    /// where the cat is in a breath (0..1: in, then out), counted while it purrs
    breath_phase: f64,
    /// how far purring has shut the mouth (0..1)
    shut: f64,
    /// the mouth sections a jet crosses before it breaks up
    jet_sections: usize,
    /// how close the listener is (1 across the room), its target and its per-sample smoothing
    close: f64,
    close_target: f64,
    close_coef: f64,
    /// the glottis now (cm²), for the source glyph
    area: f64,
    fs: f64,
}

impl Cat {
    fn new(fs: f64) -> Self {
        let a = mammal_airway(fs, MammalSpec { trachea: TRACHEA, pharynx: PHARYNX, oral: ORAL, nasal: Some(NASAL), wall: None, wall_radiation: Some(0.6), coarse: None, nasal_radiates: None }, |_, _| {});
        let nasal = a.nasal.expect("cat has a nose");
        let velum = a.velum.expect("cat has a soft palate");
        let tubes = vec![("trachea", a.trachea), ("pharynx", a.pharynx), ("oral", a.oral), ("nasal", nasal)];
        let jet_sections = js_round(JET_LENGTH / (ORAL.0 / a.oral.n as f64)) as usize;
        Cat { a, tubes, velum, jet_sections, twitch_phase: 0.0, twitch_step: 0.0, twitch_area: 0.0, breath_phase: 0.0, shut: 0.0, close: 1.0, close_target: 1.0, close_coef: 1.0 - exp(-1.0 / (0.02 * fs)), area: 0.0, fs }
    }
}

fn create(fs: f64) -> Box<dyn CreatureSim> {
    Box::new(Cat::new(fs))
}

/// The breath while purring, −1 (in) to 1 (out) over one cycle: each half rises, holds and
/// falls, and it is still for a moment where the breath turns.
fn breath(phase: f64) -> f64 {
    let (u, sign) = if phase < BREATH_IN { (phase / BREATH_IN, -1.0) } else { ((phase - BREATH_IN) / (1.0 - BREATH_IN), 1.0) };
    let x = 2.0 * u - 1.0;
    sign * (1.0 - x * x * x * x)
}

/// How far the folds are apart through one pulse of a purr (0..1 at cycle phase `ph`): they part
/// slowly and snap shut, and the snap is what rings the throat.
#[inline]
fn puff(ph: f64) -> f64 {
    if ph >= PUFF_DUTY {
        return 0.0;
    }
    let u = ph / PUFF_DUTY;
    if u < 0.7 { 0.5 - 0.5 * cos((PI * u) / 0.7) } else { cos((0.5 * PI * (u - 0.7)) / 0.3) }
}

/// The mouth's cross-section (cm²) at `u` from the throat (0) to the teeth (1). The jaw hinges at
/// the back, so the gap under the palate grows toward the teeth; the tongue is a hump that sits
/// under the hard palate when forward (the "mi" of a meow) and under the soft palate when back
/// (the "ow"), and drops as the jaw opens, unless the cat hisses: then it is pressed up to leave
/// a slit; the lips and teeth are the last 0.8 cm.
fn mouth_area(u: f64, mouth: f64, tongue: f64, hiss: f64) -> f64 {
    let open = 0.35 + 1.9 * mouth * (0.3 + 0.7 * u);
    let x = (u - (0.2 + 0.55 * tongue)) / 0.2;
    let rest = 0.8 * (1.0 - 0.55 * mouth);
    let hump = rest + (HISS_SLIT - rest) * hiss;
    let a = open * (1.0 - hump * exp(-x * x));
    if u > 0.86 { js_min(a, GAPE * mouth * sqrt(mouth)) } else { a }
}

impl CreatureSim for Cat {
    fn net(&self) -> Option<&AirwayNetwork> {
        Some(&self.a.net)
    }
    fn net_mut(&mut self) -> Option<&mut AirwayNetwork> {
        Some(&mut self.a.net)
    }
    fn tubes(&self) -> &[(&'static str, Tube)] {
        &self.tubes
    }
    fn gain(&self) -> f64 {
        1.5e-7
    }
    fn control(&mut self, p: &[f64], gate: f64, dt: f64) {
        let (purr, hiss) = (p[PURR], p[HISS]);
        // a purring cat breathes: in through the nose, then out, the folds parting all the while
        let breathing = smoothstep(0.5, 0.9, purr);
        if gate < 0.02 {
            self.breath_phase = 0.0;
        } else {
            self.breath_phase += dt / BREATH;
            self.breath_phase -= self.breath_phase.floor();
        }
        let wind = breath(self.breath_phase);
        let a = &mut self.a;
        a.net.set_fixed_pressure(a.lungs, p[PRESSURE] * CMH2O * gate * ((1.0 - breathing) + breathing * PURR_PRESSURE * wind));
        self.twitch_step = (p[PURR_RATE] * (1.0 + BREATH_RATE * breathing * wind)) / self.fs;
        self.twitch_area = PUFF_AREA * purr * purr * (1.0 - hiss);
        // a purring cat's mouth is shut. It shuts and opens gently (a quick jaw pushes air: a thump),
        // and the ear is against the cat only once it has shut
        self.shut += (breathing - self.shut) * (1.0 - exp(-dt / 0.08));
        self.close_target = 1.0 + (PURR_CLOSE - 1.0) * breathing * smoothstep(0.85, 1.0, self.shut);
        // from a clear voice (purr 0) through a voice the purr's pulse chops into a trill (0.5) to none
        let flutter = js_min(1.0, 2.0 * purr);
        let g = &mut a.glottis_src.params;
        g.f0 = p[PITCH];
        // stretched for a high note the folds swing less far, slack for a low one further (without
        // this, 1600 Hz at full breath peaked at 0.98 of full scale, and a 150 Hz growl was a
        // quarter as loud as a meow)
        let reach = clamp(sqrt(TALKING_PITCH / p[PITCH]), 0.6, 1.5);
        g.amplitude = FOLD_AREA * reach * (1.0 - hiss) * (1.0 - purr) * (1.0 - flutter + flutter * (0.5 - 0.5 * cos(TAU * self.twitch_phase)));
        g.abduction = HISS_AREA * hiss;
        g.chink = 0.002;
        // a harder breath claps the folds shut harder: louder, and brighter with it
        let press = smoothstep(3.0, 12.0, p[PRESSURE]);
        g.open_quotient = 0.6 - 0.05 * press;
        g.skew = 0.65 + 0.15 * press;
        g.closure_sharpness = 0.3 + 0.7 * press;
        g.effort = 0.5 + 0.5 * press;
        g.chaos = p[ROUGH];
        g.jitter = 0.004 + 0.03 * p[ROUGH];
        g.shimmer = 0.03 + 0.2 * p[ROUGH];
        g.drift = 5.0;
        let (mouth, tongue) = (p[MOUTH] * (1.0 - self.shut), p[TONGUE]);
        // the tongue's root follows it: back, it crowds the throat
        let ph_areas = fill_areas(a.pharynx, |u| 0.9 - 0.45 * (1.0 - tongue) * u, &mut a.phar_area);
        a.net.set_tube_targets(a.pharynx, ph_areas, None, None);
        let or_areas = fill_areas(a.oral, |u| mouth_area(u, mouth, tongue, hiss), &mut a.oral_area);
        a.net.set_tube_targets(a.oral, or_areas, None, None);
        // the soft palate hangs open with the mouth shut (a cat breathes, purrs and trills through
        // its nose), lifts as the jaw drops, and shuts for a hiss
        a.net.set_port_target(self.velum, 0.2 * (1.0 - 0.75 * mouth) * (1.0 - hiss));
        // turbulence: breath through the glottis, and the jet from the mouth's narrowest point,
        // which turns to noise where it breaks up, a centimetre on (one section on, the same
        // noise came out flat to 24 kHz: a sizzle, where a hiss sits at 4-6 kHz)
        let slit = (0..a.oral.n - 1).fold(0, |m, i| if a.oral_area[i] < a.oral_area[m] { i } else { m });
        let breakup = slit + self.jet_sections;
        a.net.set_noise_sources(&[
            NoiseSpec::new(a.glottis, a.pharynx.branch(1.min(a.pharynx.n - 2)), a.trachea.branch(a.trachea.n - 2), 3e-6).band(4000.0, 250.0),
            NoiseSpec::new(a.oral.branch(slit), a.oral.branch((a.oral.n - 2).min(breakup)), a.oral.branch(slit.saturating_sub(1)), JET).band(4000.0, 700.0),
        ]);
    }
    #[inline]
    fn sample(&mut self) -> f64 {
        self.twitch_phase += self.twitch_step;
        if self.twitch_phase >= 1.0 {
            self.twitch_phase -= 1.0;
        }
        let a = &mut self.a;
        let p = &a.net.p;
        // the folds vibrate, and part for a purr's pulse, whichever way the air is going
        let dp = (p[a.trachea.last_node()] - p[a.pharynx.first_node()]).abs();
        self.area = a.glottis_src.next(dp) + self.twitch_area * puff(self.twitch_phase);
        a.net.set_port_area(a.glottis, self.area);
        self.close += self.close_coef * (self.close_target - self.close);
        a.net.step() * self.close
    }
    fn source_state(&self, out: &mut Vec<(&'static str, f64)>) {
        out.clear();
        out.push(("glottis", js_min(1.0, self.area / FOLD_AREA)));
    }
}

pub fn def() -> CreatureDef {
    let k = CallKey::new;
    CreatureDef {
        id: "cat",
        name: "House cat",
        latin: "Felis catus",
        group: CreatureGroup::Mammal,
        blurb: "A small larynx under a mouth that opens and closes: the meow's wah is the jaw. The purr is the folds parting 25 times a second through the breath in and the breath out.",
        role: "Wah leads, talking pads, purring tremolo bass, hissing noise sweeps",
        oversample: 2,
        gate_times: Some(GateTimes { attack: 0.02, release: 0.05 }),
        params: vec![
            ParamDef::new("pitch", "Pitch", 80.0, 1600.0, TALKING_PITCH).unit("Hz").log().smooth(0.012).pitch(),
            // (the held voice starts at 600 Hz with the mouth at 0.3, where the fundamental is 8 dB
            // under the second harmonic. At 560 Hz and 0.5 that harmonic sat on the mouth's
            // resonance, 20 dB over the fundamental, and the note read as the octave above.)
            ParamDef::new("mouth", "Mouth opening", 0.0, 1.0, 0.3).smooth(0.02),
            ParamDef::new("purr", "Voice ↔ purr", 0.0, 1.0, 0.0).smooth(0.03),
            ParamDef::new("pressure", "Breath pressure", 2.0, 20.0, 9.0).unit("cmH₂O").smooth(0.02),
            ParamDef::new("tongue", "Tongue (back ↔ front)", 0.0, 1.0, 0.5).smooth(0.02),
            ParamDef::new("purrRate", "Purr rate", 15.0, 45.0, 25.0).unit("Hz").log().smooth(0.03),
            ParamDef::new("rough", "Roughness", 0.0, 1.0, 0.0).smooth(0.05),
            ParamDef::new("hiss", "Hiss (open glottis)", 0.0, 1.0, 0.0).smooth(0.03),
        ],
        calls: vec![
            CallDef::new(
                "meow",
                "Meow",
                "Mi-a-ow: the pitch rises and falls while the mouth opens and closes",
                Some(720.0),
                // (the top is 720 Hz and not lower so that its second harmonic passes above the
                // open mouth's 1.25 kHz resonance: at 650 Hz it sat on it, 22 dB over a
                // fundamental that then all but vanished)
                vec![
                    k(0.0, &[("gate", 1.0), ("purr", 0.0), ("hiss", 0.0), ("rough", 0.0), ("pressure", 6.0), ("pitch", 520.0), ("mouth", 0.0), ("tongue", 0.95)]),
                    k(0.1, &[("pitch", 600.0), ("mouth", 0.1)]),
                    k(0.32, &[("pressure", 12.0), ("pitch", 720.0), ("mouth", 0.95), ("tongue", 0.55)]),
                    k(0.62, &[("pressure", 10.0), ("pitch", 640.0), ("mouth", 0.5), ("tongue", 0.2)]),
                    k(0.92, &[("gate", 1.0), ("pressure", 5.0), ("pitch", 430.0), ("mouth", 0.05), ("tongue", 0.05)]),
                    k(0.98, &[("gate", 0.0)]),
                    k(1.15, &[("mouth", 0.05)]),
                ],
            ),
            CallDef::new(
                "trill",
                "Trill",
                "Mrrp? A rising chirrup through the nose, rolled at the purr's rate",
                Some(600.0),
                vec![
                    k(0.0, &[("gate", 1.0), ("purr", 0.5), ("hiss", 0.0), ("rough", 0.0), ("pressure", 11.0), ("purrRate", 30.0), ("pitch", 300.0), ("mouth", 0.0), ("tongue", 0.6)]),
                    k(0.36, &[("pitch", 540.0), ("purr", 0.4), ("mouth", 0.0)]).curve(Curve::Exp),
                    k(0.5, &[("gate", 1.0), ("pitch", 600.0), ("purr", 0.15), ("mouth", 0.12)]),
                    k(0.54, &[("gate", 0.0)]),
                    k(0.7, &[("mouth", 0.12)]),
                ],
            ),
            CallDef::new(
                "purr",
                "Purr",
                "Four breaths in and out through the nose, the folds parting 25 times a second",
                None,
                vec![
                    k(0.0, &[("gate", 0.0), ("purr", 1.0), ("hiss", 0.0), ("rough", 0.0), ("pressure", 9.0), ("purrRate", 25.0)]),
                    k(0.1, &[("gate", 0.0)]),
                    k(0.13, &[("gate", 1.0)]),
                    k(0.13 + 4.0 * BREATH, &[("gate", 1.0)]),
                    k(0.17 + 4.0 * BREATH, &[("gate", 0.0)]),
                    k(0.35 + 4.0 * BREATH, &[("purr", 1.0)]),
                ],
            ),
            CallDef::new(
                "hiss",
                "Hiss",
                "Mouth wide, glottis open, the tongue arched: a jet over the teeth",
                None,
                vec![k(0.0, &[("gate", 0.0), ("hiss", 1.0), ("purr", 0.0), ("pressure", 18.0), ("mouth", 0.75), ("tongue", 0.6)]), k(0.08, &[("gate", 0.0)]), k(0.14, &[("gate", 1.0)]), k(0.6, &[("gate", 1.0), ("pressure", 14.0)]), k(0.95, &[("gate", 0.0)]), k(1.15, &[("hiss", 1.0)])],
            ),
        ],
        events: vec![],
        create,
    }
    .resolved()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airway::frequency::{find_peaks, linspace, transfer_function};

    /// The tract's resonances (Hz) with the mouth and tongue here, from the frequency-domain model.
    fn formants(mouth: f64, tongue: f64) -> Vec<f64> {
        let mut c = Cat::new(96000.0);
        let mut p: Vec<f64> = def().params.iter().map(|d| d.default).collect();
        p[MOUTH] = mouth;
        p[TONGUE] = tongue;
        c.control(&p, 0.0, 64.0 / 96000.0);
        c.a.net.snap_to_targets();
        let fr = linspace(100.0, 8000.0, 1581);
        let h = transfer_function(&c.a.net.linear_elements(), c.a.pharynx.first_node(), &fr, &[c.a.glottis]);
        find_peaks(&fr, &h.mag, 150.0).into_iter().map(|k| k.freq).collect()
    }

    /// Run the simulation as the voice does (a control step every 128 samples) for `seconds`
    /// with these settings and the breath on; returns the flow through the glottis (cm³/s, out
    /// of the lungs positive) and the output, per sample.
    fn run(c: &mut Cat, p: &[f64], seconds: f64) -> (Vec<f64>, Vec<f64>) {
        let (mut flow, mut out) = (Vec::new(), Vec::new());
        for _ in 0..(seconds * c.fs / 128.0) as usize {
            c.control(p, 1.0, 128.0 / c.fs);
            c.a.net.begin_block(128);
            for _ in 0..128 {
                out.push(c.sample());
                flow.push(c.a.net.u[c.a.glottis]);
            }
        }
        (flow, out)
    }

    fn mean(x: &[f64]) -> f64 {
        x.iter().sum::<f64>() / x.len() as f64
    }

    #[test]
    fn the_jaw_is_the_wah() {
        // the first resonance climbs as the mouth opens, wherever the tongue is: from under
        // 0.7 kHz with the lips shut (through the nose) to over 1.2 kHz at full gape
        for tongue in [0.0, 0.5, 1.0] {
            let f1 = [0.0, 0.08, 0.2, 0.4, 0.6, 0.8, 1.0].map(|mouth| formants(mouth, tongue)[0]);
            assert!(f1.windows(2).all(|w| w[1] > w[0] + 30.0), "tongue {tongue}: F1 by mouth opening {f1:?}");
            assert!(f1[0] > 600.0 && f1[0] < 720.0, "tongue {tongue}: F1 with the mouth shut {}", f1[0]);
            assert!(f1[6] > 1150.0 && f1[6] < 1400.0, "tongue {tongue}: F1 with the mouth wide {}", f1[6]);
        }
    }

    #[test]
    fn the_tongue_moves_the_second_resonance() {
        // behind lips that have just parted: 1.6 kHz with the tongue back, 2.3 kHz with it forward
        let f2 = [0.0, 0.25, 0.5, 0.75, 1.0].map(|tongue| formants(0.02, tongue)[1]);
        assert!(f2.windows(2).all(|w| w[1] > w[0]), "F2 by tongue position {f2:?}");
        assert!(f2[0] < 1700.0 && f2[4] > 2200.0, "F2 by tongue position {f2:?}");
    }

    #[test]
    fn a_purring_cat_breathes_in_then_out_with_its_mouth_shut() {
        let mut c = Cat::new(96000.0);
        let mut p: Vec<f64> = def().params.iter().map(|d| d.default).collect();
        p[PURR] = 1.0;
        c.control(&p, 0.0, 128.0 / c.fs);
        c.a.net.snap_to_targets();
        let (flow, _) = run(&mut c, &p, 2.0 * BREATH);
        let at = |t: f64| (t * c.fs) as usize;
        for breath in 0..2 {
            let t0 = breath as f64 * BREATH;
            // air goes in through the glottis for the first 0.66 s of a breath, then out
            let (inward, outward) = (mean(&flow[at(t0 + 0.1)..at(t0 + 0.6)]), mean(&flow[at(t0 + 0.72)..at(t0 + 1.18)]));
            assert!(inward < -3.0 && outward > 3.0, "breath {breath}: mean flow {inward:.1} cm³/s in, {outward:.1} out");
            // and it is puffs, not a stream: the folds are shut for most of each cycle
            let shut = flow[at(t0 + 0.1)..at(t0 + 0.6)].iter().filter(|u| u.abs() < 0.2 * inward.abs()).count() as f64 / (at(0.5) as f64);
            assert!(shut > 0.5, "breath {breath}: the glottis passes next to nothing {:.0}% of the time", shut * 100.0);
        }
        // the lips are sealed (though the mouth setting is the default, part open), so it is the nose that sounds
        let lips = c.a.net.area[c.a.oral.last_node()];
        assert!(lips < 1e-3, "lip area while purring {lips}");
    }

    #[test]
    fn the_folds_stop_for_a_hiss_and_the_tongue_leaves_a_slit() {
        let mut c = Cat::new(96000.0);
        let mut p: Vec<f64> = def().params.iter().map(|d| d.default).collect();
        p[HISS] = 1.0;
        p[MOUTH] = 0.75;
        p[TONGUE] = 0.6;
        p[PRESSURE] = 18.0;
        c.control(&p, 0.0, 128.0 / c.fs);
        c.a.net.snap_to_targets();
        run(&mut c, &p, 0.5);
        // the glottis is held open and still
        assert!((c.area - HISS_AREA).abs() < 0.01, "glottis {} cm²", c.area);
        assert!(c.a.glottis_src.voicing < 0.01, "voicing {}", c.a.glottis_src.voicing);
        // the narrowest point of the mouth is the tongue's slit, well under the open glottis
        let slit = c.a.oral_area.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!(slit > 0.03 && slit < 0.1, "slit {slit} cm²");
        // and its jet is turbulent: the second noise source is the jet's
        assert!(c.a.net.noise_sources()[1].level > 100.0, "jet noise {}", c.a.net.noise_sources()[1].level);
    }
}
