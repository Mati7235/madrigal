use std::env;
use std::fs;
use std::path::Path;

use madrigal_files::{atomic_write_bytes, atomic_write_text};

fn tmp_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if name.ends_with(".tmp") {
            out.push(name);
        }
    }
    out
}

#[test]
fn writes_are_all_or_nothing() {
    let root = tempfile::tempdir().expect("tempdir");
    env::set_var("MADRIGAL_PROFILE_ROOT", root.path());
    let dir = root.path().join("data").join("madrigal");
    fs::create_dir_all(&dir).unwrap();

    let target = dir.join("doc.txt");
    atomic_write_text(&target, "primera").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "primera");
    atomic_write_bytes(&dir.join("bin.dat"), &[0, 1, 2, 255]).unwrap();
    assert_eq!(fs::read(dir.join("bin.dat")).unwrap(), vec![0, 1, 2, 255]);
    assert!(tmp_files(&dir).is_empty(), "quedó temporal tras éxito");

    let blocker = dir.join("blocker");
    atomic_write_text(&blocker, "x").unwrap();
    let bad = blocker.join("doc.txt");
    assert!(atomic_write_text(&bad, "parcial").is_err());
    assert!(tmp_files(&dir).is_empty(), "quedó temporal tras falla");
    assert_eq!(fs::read_to_string(&target).unwrap(), "primera");
    assert!(!bad.exists(), "quedó archivo parcial");
}
