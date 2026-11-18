use std::path::{Path, PathBuf};
use std::process::Command;

use madrigal_shazam::{Signature, SignatureGenerator, available_tool, crc32, read_wav_mono16k};

fn ffmpeg(args: &[&str]) {
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg failed for {args:?}");
}

fn tone_seq(path: &Path, freqs: &[u32]) {
    let mut args: Vec<String> = Vec::new();
    for freq in freqs {
        args.push("-f".to_owned());
        args.push("lavfi".to_owned());
        args.push("-i".to_owned());
        args.push(format!("sine=frequency={freq}:duration=1:sample_rate=16000"));
    }
    let mut mix = String::new();
    for i in 0..freqs.len() {
        mix.push_str(&format!("[{i}]"));
    }
    mix.push_str(&format!("concat=n={}:v=0:a=1", freqs.len()));
    args.push("-filter_complex".to_owned());
    args.push(mix);
    args.push("-ac".to_owned());
    args.push("1".to_owned());
    args.push("-ar".to_owned());
    args.push("16000".to_owned());
    args.push(path.to_str().unwrap().to_owned());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    ffmpeg(&refs);
}

fn signature_of(path: &Path) -> Option<Signature> {
    let pcm = read_wav_mono16k(path).expect("wav reads");
    let mut gen = SignatureGenerator::new();
    gen.feed_pcm16_mono(&pcm);
    gen.next_signature()
}

fn wav_path(root: &Path, name: &str) -> PathBuf {
    root.join(name)
}

fn native_root() -> Option<tempfile::TempDir> {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        println!("SKIP shazam_signature native (ffmpeg missing)");
        return None;
    }
    Some(tempfile::tempdir().expect("tempdir"))
}

#[test]
fn same_pcm_gives_identical_signature() {
    let Some(root) = native_root() else {
        return;
    };
    let wav = wav_path(root.path(), "tone.wav");
    tone_seq(&wav, &[440, 660, 550, 880]);
    let first = signature_of(&wav).expect("mix yields a signature");
    let second = signature_of(&wav).expect("mix yields a signature");
    assert_eq!(first.as_bytes(), second.as_bytes());
    assert_eq!(first.uri, second.uri);

    let pcm = read_wav_mono16k(&wav).expect("wav reads");
    let mid = pcm.len() / 2;
    let mut chunked = SignatureGenerator::new();
    chunked.feed_pcm16_mono(&pcm[..mid]);
    chunked.feed_pcm16_mono(&pcm[mid..]);
    let third = chunked.next_signature().expect("chunked yields");
    assert_eq!(first.as_bytes(), third.as_bytes());
}

#[test]
fn different_audio_gives_different_signature() {
    let Some(root) = native_root() else {
        return;
    };
    let low = wav_path(root.path(), "low.wav");
    let high = wav_path(root.path(), "high.wav");
    tone_seq(&low, &[440, 660, 550, 880]);
    tone_seq(&high, &[880, 550, 660, 440]);
    let low_sig = signature_of(&low).expect("low mix yields");
    let high_sig = signature_of(&high).expect("high mix yields");
    assert_ne!(low_sig.uri, high_sig.uri);
    assert_ne!(low_sig.as_bytes(), high_sig.as_bytes());
}

#[test]
fn known_mix_signature_is_stable() {
    let Some(root) = native_root() else {
        return;
    };
    let wav = wav_path(root.path(), "pinned.wav");
    tone_seq(&wav, &[440, 660, 550, 880]);
    let sig = signature_of(&wav).expect("pinned mix yields");
    assert_eq!(sig.sample_duration_ms, 4000);
    assert_eq!(sig.as_bytes().len(), 292);
    assert_eq!(crc32(sig.as_bytes()), 0xea9b_e15d);
    assert!(sig.uri.starts_with(
        "data:audio/vnd.shazam.sig;base64,gCX+yu4WzA70AAAAAJwRlAAAAAAAAAA"
    ));
    let _ = available_tool();
}

#[test]
fn truncated_and_garbage_audio_fail_cleanly() {
    let Some(root) = native_root() else {
        return;
    };
    let wav = wav_path(root.path(), "tone.wav");
    tone_seq(&wav, &[440, 660, 550, 880]);
    let bytes = std::fs::read(&wav).expect("wav bytes");

    let cut = wav_path(root.path(), "cut.wav");
    std::fs::write(&cut, &bytes[..20]).expect("write cut");
    assert!(read_wav_mono16k(&cut).is_err());

    let garbage = wav_path(root.path(), "garbage.wav");
    std::fs::write(&garbage, [0xABu8; 4096]).expect("write garbage");
    assert!(read_wav_mono16k(&garbage).is_err());

    let empty = wav_path(root.path(), "empty.wav");
    std::fs::write(&empty, []).expect("write empty");
    assert!(read_wav_mono16k(&empty).is_err());

    let short = wav_path(root.path(), "short.wav");
    std::fs::write(&short, &bytes[..bytes.len() / 32]).expect("write short");
    match read_wav_mono16k(&short) {
        Err(_) => {}
        Ok(pcm) => {
            let mut gen = SignatureGenerator::new();
            gen.feed_pcm16_mono(&pcm);
            let _ = gen.next_signature();
        }
    }
}
