//! The demo songs as the app plays them (the TypeScript fixtures hold only the two songs that
//! engine had). The app gives note i the syllable `i % syllables`, so a song with fewer syllables
//! than notes sings its opening words again at its end, and one with more stops short of its last
//! words: every song must have exactly one note per syllable. And it sends a song as one
//! schedule, which takes 4000 gestures and silently drops the rest.
use chimera::engine::MAX_SCHEDULE;
use chimera::phonetics::english::EnglishG2P;
use chimera::phonetics::sing::{SongOptions, SungNote, plan_song};
use chimera::phonetics::songs::DEMO_SONGS;
use chimera::phonetics::syllables::lyrics_to_syllables;
use chimera::phonetics::Syllable;
use std::collections::HashSet;
use std::io::Read;

/// The pronouncing dictionary, and the words it has.
fn g2p() -> (EnglishG2P, HashSet<String>) {
    let path = format!("{}/../../public/data/cmudict.txt.gz", env!("CARGO_MANIFEST_DIR"));
    let gz = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut text = String::new();
    flate2::read::GzDecoder::new(&gz[..]).read_to_string(&mut text).unwrap();
    let mut g = EnglishG2P::new();
    g.load_dict(&text);
    (g, text.lines().filter_map(|l| l.split_once('\t')).map(|(w, _)| w.to_string()).collect())
}

#[test]
fn every_demo_song_has_one_note_per_syllable_and_fits_one_schedule() {
    let (g, dictionary) = g2p();
    for song in DEMO_SONGS {
        let syl: Vec<Syllable> = lyrics_to_syllables(song.lyrics, &g);
        let words = syl.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
        assert_eq!(syl.len(), song.notes.len(), "{:?}: {} syllables for {} notes: {words}", song.name, syl.len(), song.notes.len());
        // every word is in the dictionary: a word spelled out by the letter-to-sound rules may
        // come out with another count on another day
        for word in song.lyrics.split(|c: char| !(c.is_alphabetic() || c == '\'')).filter(|w| !w.is_empty()) {
            assert!(dictionary.contains(&word.to_lowercase()), "{:?}: {word:?} is not in the dictionary", song.name);
        }
        for (body, octave) in [("male", song.octave.male), ("female", song.octave.female), ("child", song.octave.child)] {
            let notes: Vec<SungNote> = song.notes.iter().zip(&syl).map(|(n, s)| SungNote { midi: n[0] + 12.0 * octave, beats: n[1], syl: s.clone() }).collect();
            let plan = plan_song(&notes, &SongOptions { body: body.into(), bpm: song.bpm, t0: 0.35, vibrato: 35.0 });
            assert!(plan.gestures.len() <= MAX_SCHEDULE, "{:?} for the {body} body: {} gestures, and a schedule takes {MAX_SCHEDULE}", song.name, plan.gestures.len());
            // sung within a voice's reach: between 80 Hz and 700 Hz
            let (lo, hi) = notes.iter().fold((f64::MAX, 0.0f64), |(lo, hi), n| (lo.min(n.midi), hi.max(n.midi)));
            assert!(lo >= 40.0 && hi <= 77.0, "{:?} for the {body} body: notes {lo} to {hi}", song.name);
            let beats: f64 = song.notes.iter().map(|n| n[1]).sum();
            assert!((plan.end - (0.35 + beats * 60.0 / song.bpm)).abs() < 0.5, "{:?}: the plan ends at {:.2} s", song.name, plan.end);
        }
    }
}
