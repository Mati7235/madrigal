use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

mod json;

use json::Json;

pub const GROUPS: [&str; 4] = ["offline", "native", "live", "visual"];
pub const DETAIL_LINES: usize = 40;

pub struct Check {
    pub name: String,
    pub group: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Pass,
    Fail,
    Skip,
}

#[derive(Debug)]
pub struct Options {
    pub group: Option<String>,
    pub only: Vec<String>,
    pub crate_name: Option<String>,
    pub max_fails: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            group: None,
            only: Vec::new(),
            crate_name: None,
            max_fails: usize::MAX,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub package: String,
    pub name: String,
    pub dir: PathBuf,
}

pub type Inventory = HashMap<String, Vec<Target>>;

pub struct Selection<'a> {
    pub check: &'a Check,
    pub targets: Vec<Target>,
}

pub struct Ran {
    pub outcome: Outcome,
    pub detail: String,
}

impl Ran {
    fn passed() -> Self {
        Self {
            outcome: Outcome::Pass,
            detail: String::new(),
        }
    }

    fn failed(detail: String) -> Self {
        Self {
            outcome: Outcome::Fail,
            detail,
        }
    }

    fn skipped() -> Self {
        Self {
            outcome: Outcome::Skip,
            detail: String::new(),
        }
    }
}

pub fn parse_args(argv: &[String]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--group" => {
                i += 1;
                let g = argv.get(i).ok_or("falta valor para --group")?.clone();
                if !GROUPS.contains(&g.as_str()) {
                    return Err(format!("grupo invalido: {g}"));
                }
                opts.group = Some(g);
            }
            "--only" => {
                i += 1;
                let raw = argv.get(i).ok_or("falta valor para --only")?;
                let names: Vec<String> = raw
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect();
                if names.is_empty() {
                    return Err("falta valor para --only".to_string());
                }
                opts.only.extend(names);
            }
            "--crate" => {
                i += 1;
                let krate = argv.get(i).ok_or("falta valor para --crate")?.trim();
                if krate.is_empty() {
                    return Err("falta valor para --crate".to_string());
                }
                opts.crate_name = Some(krate.to_string());
            }
            "--max-fails" => {
                i += 1;
                let n: usize = argv
                    .get(i)
                    .ok_or("falta valor para --max-fails")?
                    .parse()
                    .map_err(|_| "max-fails debe ser un entero >= 1")?;
                if n < 1 {
                    return Err("max-fails debe ser un entero >= 1".to_string());
                }
                opts.max_fails = n;
            }
            other => return Err(format!("argumento desconocido: {other}")),
        }
        i += 1;
    }
    Ok(opts)
}

fn strip_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => return line[..idx].trim_end(),
            _ => {}
        }
    }
    line
}

fn parse_quoted(value: &str) -> Option<String> {
    let v = value.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
    {
        Some(v[1..v.len() - 1].to_string())
    } else {
        None
    }
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.split_once('=')?;
    Some((k.trim(), v.trim()))
}

pub fn parse_registry(text: &str) -> Vec<Check> {
    let mut checks = Vec::new();
    let mut name: Option<String> = None;
    let mut group: Option<String> = None;
    let mut flush = |name: &mut Option<String>, group: &mut Option<String>| {
        if let (Some(n), Some(g)) = (name.take(), group.take()) {
            checks.push(Check { name: n, group: g });
        }
    };
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(inner) = line.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
            flush(&mut name, &mut group);
            if inner.trim() == "check" {
                name = None;
                group = None;
            }
            continue;
        }
        if let Some(inner) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            flush(&mut name, &mut group);
            if let Some(n) = inner.trim().strip_prefix("checks.") {
                name = Some(n.trim().to_string());
                group = None;
            }
            continue;
        }
        if let Some((k, v)) = split_key_value(line) {
            match k {
                "name" => {
                    if let Some(val) = parse_quoted(v) {
                        name = Some(val);
                    }
                }
                "group" => {
                    if let Some(val) = parse_quoted(v) {
                        group = Some(val);
                    }
                }
                _ => {}
            }
        }
    }
    flush(&mut name, &mut group);
    checks
}

pub fn parse_package_name(text: &str) -> Option<String> {
    let mut section = String::new();
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line.to_string();
            continue;
        }
        if section == "[package]" {
            if let Some((k, v)) = split_key_value(line) {
                if k == "name" {
                    return parse_quoted(v);
                }
            }
        }
    }
    None
}

pub fn parse_test_targets(text: &str) -> Vec<(String, String)> {
    let mut targets = Vec::new();
    let mut section = String::new();
    let mut name: Option<String> = None;
    let mut path: Option<String> = None;
    let mut flush = |name: &mut Option<String>, path: &mut Option<String>| {
        if let Some(n) = name.take() {
            let p = path.take().unwrap_or_else(|| format!("tests/{n}.rs"));
            targets.push((n, p));
        }
    };
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            flush(&mut name, &mut path);
            section = line.to_string();
            continue;
        }
        if section == "[[test]]" {
            if let Some((k, v)) = split_key_value(line) {
                match k {
                    "name" => name = parse_quoted(v),
                    "path" => path = parse_quoted(v),
                    _ => {}
                }
            }
        }
    }
    flush(&mut name, &mut path);
    targets
}

pub fn discover(root: &Path) -> Inventory {
    let mut inventory: Inventory = HashMap::new();
    let Ok(entries) = std::fs::read_dir(root.join("crates")) else {
        return inventory;
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        let Some(package) = parse_package_name(&text) else {
            continue;
        };
        for (target, rel) in parse_test_targets(&text) {
            let Some(name) = target.strip_prefix("check_") else {
                continue;
            };
            if !dir.join(&rel).is_file() {
                continue;
            }
            inventory.entry(name.to_string()).or_default().push(Target {
                package: package.clone(),
                name: target,
                dir: dir.clone(),
            });
        }
    }
    for targets in inventory.values_mut() {
        targets.sort_by(|a, b| a.package.cmp(&b.package));
    }
    inventory
}

pub fn select<'a>(
    checks: &'a [Check],
    opts: &Options,
    inventory: &Inventory,
) -> Vec<Selection<'a>> {
    checks
        .iter()
        .filter(|c| opts.group.as_ref().is_none_or(|g| &c.group == g))
        .filter(|c| opts.only.is_empty() || opts.only.contains(&c.name))
        .filter_map(|check| {
            let owners = inventory.get(&check.name).cloned().unwrap_or_default();
            let targets: Vec<Target> = match &opts.crate_name {
                Some(package) => owners
                    .into_iter()
                    .filter(|target| &target.package == package)
                    .collect(),
                None => owners,
            };
            if opts.crate_name.is_some() && targets.is_empty() {
                return None;
            }
            Some(Selection { check, targets })
        })
        .collect()
}

pub fn tail_lines(text: &str, limit: usize) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    let start = lines.len().saturating_sub(limit);
    lines[start..].join("\n")
}

pub struct Artifact {
    pub target: String,
    pub executable: Option<PathBuf>,
}

pub struct Diagnostic {
    pub target: Option<String>,
    pub rendered: String,
}

pub struct BuildReport {
    pub artifacts: Vec<Artifact>,
    pub diagnostics: Vec<Diagnostic>,
    pub finished: Option<bool>,
}

fn field<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    value.get(key)
}

fn text(value: &Json, key: &str) -> Option<String> {
    field(value, key).and_then(Json::as_str).map(str::to_owned)
}

pub fn parse_report(stdout: &str) -> BuildReport {
    let mut artifacts = Vec::new();
    let mut diagnostics = Vec::new();
    let mut finished = None;
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(value) = json::parse(line) else {
            continue;
        };
        match text(&value, "reason").as_deref() {
            Some("compiler-artifact") => {
                let Some(target) = field(&value, "target") else {
                    continue;
                };
                let Some(name) = text(target, "name") else {
                    continue;
                };
                let is_test = field(target, "kind")
                    .map(Json::strings)
                    .is_some_and(|kinds| kinds.iter().any(|kind| kind == "test"));
                if !is_test {
                    continue;
                }
                artifacts.push(Artifact {
                    target: name,
                    executable: text(&value, "executable")
                        .filter(|path| !path.is_empty())
                        .map(PathBuf::from),
                });
            }
            Some("compiler-message") => {
                let Some(message) = field(&value, "message") else {
                    continue;
                };
                if text(message, "level").as_deref() != Some("error") {
                    continue;
                }
                let rendered = text(message, "rendered")
                    .or_else(|| text(message, "message"))
                    .unwrap_or_default();
                diagnostics.push(Diagnostic {
                    target: field(&value, "target").and_then(|t| text(t, "name")),
                    rendered,
                });
            }
            Some("build-finished") => {
                finished = field(&value, "success").and_then(Json::as_bool);
            }
            _ => {}
        }
    }
    BuildReport {
        artifacts,
        diagnostics,
        finished,
    }
}

fn cargo_command() -> Command {
    let mut command = Command::new(cargo());
    // cargo run exports CARGO_* to build scripts, which hash them and force rebuilds
    for (key, _) in std::env::vars() {
        if key == "CARGO_MANIFEST_DIR"
            || key == "CARGO_MANIFEST_PATH"
            || key == "CARGO_PRIMARY_PACKAGE"
            || key == "CARGO_TARGET_TMPDIR"
            || key == "CARGO_CRATE_NAME"
            || key.starts_with("CARGO_PKG_")
            || key.starts_with("CARGO_BIN_EXE_")
        {
            command.env_remove(key);
        }
    }
    command
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn build_error(report: &BuildReport, stderr: &str) -> String {
    if !report.diagnostics.is_empty() {
        let rendered: Vec<&str> = report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.rendered.as_str())
            .filter(|text| !text.trim().is_empty())
            .collect();
        if !rendered.is_empty() {
            return tail_lines(&rendered.join("\n"), DETAIL_LINES);
        }
    }
    let text = tail_lines(stderr, DETAIL_LINES);
    if text.is_empty() {
        "build failed without diagnostics".to_string()
    } else {
        text
    }
}

struct CrateBuild {
    bins: HashMap<String, PathBuf>,
    errors: HashMap<String, String>,
}

fn build_crate(root: &Path, package: &str, targets: &[String]) -> CrateBuild {
    let mut state = CrateBuild {
        bins: HashMap::new(),
        errors: HashMap::new(),
    };
    let mut command = cargo_command();
    command
        .current_dir(root)
        .args(["test", "--no-run", "--message-format=json", "-p", package]);
    for target in targets {
        command.args(["--test", target]);
    }
    let output = match command.output() {
        Ok(output) => output,
        Err(err) => {
            for target in targets {
                state
                    .errors
                    .insert(target.clone(), format!("cargo unavailable: {err}"));
            }
            return state;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = parse_report(&stdout);
    for artifact in &report.artifacts {
        if let Some(executable) = &artifact.executable {
            state
                .bins
                .insert(artifact.target.clone(), executable.clone());
        }
    }
    if !output.status.success() || report.finished == Some(false) {
        let fallback = build_error(&report, &stderr);
        for diagnostic in &report.diagnostics {
            let slot = diagnostic
                .target
                .as_ref()
                .filter(|name| targets.iter().any(|target| target == *name));
            let slot = match slot {
                Some(name) => name.clone(),
                None => continue,
            };
            let entry = state.errors.entry(slot).or_default();
            if !diagnostic.rendered.trim().is_empty() {
                if !entry.is_empty() {
                    entry.push('\n');
                }
                entry.push_str(diagnostic.rendered.trim_end());
            }
        }
        for target in targets {
            if !state.bins.contains_key(target) && !state.errors.contains_key(target) {
                state.errors.insert(target.clone(), fallback.clone());
            }
        }
    }
    state
}

fn build_one(root: &Path, package: &str, target: &str) -> Result<PathBuf, String> {
    let output = cargo_command()
        .current_dir(root)
        .args([
            "test",
            "--no-run",
            "--message-format=json",
            "-p",
            package,
            "--test",
            target,
        ])
        .output()
        .map_err(|err| format!("cargo unavailable: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = parse_report(&stdout);
    if let Some(executable) = report
        .artifacts
        .iter()
        .find(|artifact| artifact.target == target)
        .and_then(|artifact| artifact.executable.clone())
    {
        if output.status.success() {
            return Ok(executable);
        }
    }
    Err(build_error(&report, &stderr))
}

fn run_binary(dir: &Path, bin: &Path) -> Result<(), String> {
    let output = Command::new(bin)
        .current_dir(dir)
        .env("CARGO_MANIFEST_DIR", dir)
        .output()
        .map_err(|err| format!("no se pudo ejecutar {}: {err}", bin.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let mut detail = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        if !detail.is_empty() && !detail.ends_with('\n') {
            detail.push('\n');
        }
        detail.push_str(&stderr);
    }
    Err(tail_lines(&detail, DETAIL_LINES))
}

pub struct Runner<'a> {
    root: &'a Path,
    plan: HashMap<String, Vec<String>>,
    built: HashMap<String, CrateBuild>,
}

impl<'a> Runner<'a> {
    pub fn new(root: &'a Path, selections: &[Selection]) -> Self {
        let mut plan: HashMap<String, Vec<String>> = HashMap::new();
        for selection in selections {
            for target in &selection.targets {
                plan.entry(target.package.clone())
                    .or_default()
                    .push(target.name.clone());
            }
        }
        for targets in plan.values_mut() {
            targets.sort();
            targets.dedup();
        }
        Self {
            root,
            plan,
            built: HashMap::new(),
        }
    }

    fn build(&mut self, package: &str) -> &CrateBuild {
        if !self.built.contains_key(package) {
            let targets = self.plan.get(package).cloned().unwrap_or_default();
            let state = build_crate(self.root, package, &targets);
            self.built.insert(package.to_string(), state);
        }
        self.built.get(package).expect("built")
    }

    pub fn run(&mut self, selection: &Selection) -> Ran {
        if selection.targets.is_empty() {
            return Ran::skipped();
        }
        let mut detail = Vec::new();
        for target in &selection.targets {
            let (bin, error) = {
                let build = self.build(&target.package);
                (
                    build.bins.get(&target.name).cloned(),
                    build.errors.get(&target.name).cloned(),
                )
            };
            let resolved = match (bin, error) {
                (Some(bin), _) => Ok(bin),
                (None, Some(error)) => Err(error),
                (None, None) => {
                    let one = build_one(self.root, &target.package, &target.name);
                    let build = self.built.get_mut(&target.package).expect("build entry");
                    match &one {
                        Ok(bin) => {
                            build.bins.insert(target.name.clone(), bin.clone());
                        }
                        Err(error) => {
                            build.errors.insert(target.name.clone(), error.clone());
                        }
                    }
                    one
                }
            };
            match resolved {
                Ok(bin) => {
                    if let Err(failure) = run_binary(&target.dir, &bin) {
                        detail.push(failure);
                    }
                }
                Err(error) => detail.push(error),
            }
        }
        if detail.is_empty() {
            Ran::passed()
        } else {
            Ran::failed(detail.join("\n"))
        }
    }
}

pub fn execute(
    selections: &[Selection],
    max_fails: usize,
    run_one: &mut dyn FnMut(&Selection) -> Ran,
) -> (Vec<String>, i32) {
    if selections.is_empty() {
        return (vec!["0 checks".to_string()], 0);
    }
    let mut lines = Vec::new();
    let (mut pass, mut fail, mut skip) = (0, 0, 0);
    for selection in selections {
        let ran = run_one(selection);
        let name = &selection.check.name;
        let group = &selection.check.group;
        match ran.outcome {
            Outcome::Pass => {
                lines.push(format!("PASS {name} {group}"));
                pass += 1;
            }
            Outcome::Fail => {
                lines.push(format!("FAIL {name} {group}"));
                fail += 1;
                if !ran.detail.trim().is_empty() {
                    lines.extend(ran.detail.lines().map(str::to_owned));
                }
            }
            Outcome::Skip => {
                lines.push(format!("SKIP {name} {group}"));
                skip += 1;
            }
        }
        if fail >= max_fails {
            lines.push(format!("corte tras {fail} fallas (--max-fails)"));
            break;
        }
    }
    lines.push(format!("{pass} passed, {fail} failed, {skip} skipped"));
    (lines, i32::from(fail > 0))
}

pub fn find_registry() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        let candidate = dir.join("checks.toml");
        if candidate.is_file() {
            return candidate;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return PathBuf::from("checks.toml"),
        }
    }
}

pub fn load_registry(path: &Path) -> Vec<Check> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_registry(&text),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
[[check]]
name = "queue"
group = "offline"

[[check]]
name = "player"
group = "native"

[[check]]
name = "stream"
group = "live"
"#;

    const MANIFEST: &str = r#"
[package]
name = "madrigal-demo"

[[test]]
name = "check_queue"
path = "tests/checks/check_queue.rs"

[[test]]
name = "check_player"

[[test]]
name = "other"
path = "tests/other.rs"
"#;

    fn fixture() -> Vec<Check> {
        parse_registry(FIXTURE)
    }

    fn target(package: &str, name: &str) -> Target {
        Target {
            package: package.to_string(),
            name: format!("check_{name}"),
            dir: PathBuf::from(format!("/repo/crates/{package}")),
        }
    }

    fn inventory() -> Inventory {
        let mut inventory = Inventory::new();
        inventory.insert("queue".to_string(), vec![target("madrigal-a", "queue")]);
        inventory.insert("player".to_string(), vec![target("madrigal-b", "player")]);
        inventory.insert("stream".to_string(), vec![]);
        inventory
    }

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn registro_vacio_informa_cero_checks() {
        for text in ["", "# solo un comentario\n", "[otra]\nclave = 1\n"] {
            let checks = parse_registry(text);
            assert!(checks.is_empty());
            let (lines, code) = execute(&[], usize::MAX, &mut |_| Ran::passed());
            assert_eq!(lines, vec!["0 checks".to_string()]);
            assert_eq!(code, 0);
        }
    }

    #[test]
    fn dummy_mixto_reporta_pass_fail_skip_y_sale_1() {
        let checks = fixture();
        let opts = Options::default();
        let selected = select(&checks, &opts, &inventory());
        assert_eq!(selected.len(), 3);
        let (lines, code) = execute(
            &selected,
            opts.max_fails,
            &mut |s| match s.check.name.as_str() {
                "queue" => Ran::passed(),
                "player" => Ran::failed("boom".to_string()),
                _ => Ran::skipped(),
            },
        );
        assert_eq!(
            lines,
            vec![
                "PASS queue offline".to_string(),
                "FAIL player native".to_string(),
                "boom".to_string(),
                "SKIP stream live".to_string(),
                "1 passed, 1 failed, 1 skipped".to_string(),
            ]
        );
        assert_eq!(code, 1);
    }

    #[test]
    fn filtros_por_grupo_y_nombre() {
        let checks = fixture();
        let inventory = inventory();
        let grouped = select(
            &checks,
            &Options {
                group: Some("offline".to_string()),
                ..Options::default()
            },
            &inventory,
        );
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].check.name, "queue");
        let single = select(
            &checks,
            &Options {
                only: vec!["stream".to_string()],
                ..Options::default()
            },
            &inventory,
        );
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].check.group, "live");
    }

    #[test]
    fn only_acepta_repetidos_y_csv() {
        let opts = parse_args(&args(&[
            "--only",
            "queue, player",
            "--only",
            "stream",
            "--only",
            "queue",
        ]))
        .unwrap();
        assert_eq!(opts.only, vec!["queue", "player", "stream", "queue"]);
        let opts = parse_args(&args(&["--only", ","])).unwrap_err();
        assert_eq!(opts, "falta valor para --only");
        assert!(parse_args(&args(&["--only"])).is_err());
    }

    #[test]
    fn crate_filtra_por_paquete_y_excluye_al_resto() {
        let opts = parse_args(&args(&["--crate", "madrigal-b"])).unwrap();
        assert_eq!(opts.crate_name.as_deref(), Some("madrigal-b"));
        assert!(parse_args(&args(&["--crate", " "])).is_err());
        assert!(parse_args(&args(&["--crate"])).is_err());
        let checks = fixture();
        let selected = select(&checks, &opts, &inventory());
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].check.name, "player");
        assert_eq!(selected[0].targets[0].package, "madrigal-b");
    }

    #[test]
    fn grupo_y_only_se_combinan_por_interseccion() {
        let opts = parse_args(&args(&["--group", "offline", "--only", "player,queue"])).unwrap();
        let checks = fixture();
        let selected = select(&checks, &opts, &inventory());
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].check.name, "queue");
    }

    #[test]
    fn sin_dueno_queda_skip() {
        let checks = fixture();
        let selected = select(
            &checks,
            &Options {
                only: vec!["stream".to_string()],
                ..Options::default()
            },
            &inventory(),
        );
        assert!(selected[0].targets.is_empty());
        let (lines, code) = execute(&selected, usize::MAX, &mut |_| Ran::skipped());
        assert_eq!(lines[0], "SKIP stream live");
        assert_eq!(code, 0);
    }

    #[test]
    fn max_fails_corta_tras_la_primera_falla() {
        let checks = fixture();
        let selected = select(&checks, &Options::default(), &inventory());
        let (lines, code) = execute(&selected, 1, &mut |s| {
            if s.check.name == "queue" {
                Ran::failed("boom".to_string())
            } else {
                Ran::passed()
            }
        });
        assert_eq!(code, 1);
        assert!(lines.iter().any(|l| l.starts_with("FAIL queue")));
        assert!(!lines.iter().any(|l| l.starts_with("PASS player")));
        assert!(lines.iter().any(|l| l.contains("--max-fails")));
    }

    #[test]
    fn tail_recorta_a_las_ultimas_lineas() {
        let text: String = (1..=50).map(|n| format!("line {n}\n")).collect();
        let tail = tail_lines(&text, 40);
        assert_eq!(tail.lines().count(), 40);
        assert_eq!(tail.lines().next(), Some("line 11"));
        assert_eq!(tail.lines().last(), Some("line 50"));
        assert_eq!(tail_lines("uno\n\n\ndos\n\n", 4), "uno\n\n\ndos");
        assert_eq!(tail_lines("", 40), "");
    }

    #[test]
    fn manifiesto_extrae_targets_de_test() {
        assert_eq!(
            parse_package_name(MANIFEST).as_deref(),
            Some("madrigal-demo")
        );
        assert_eq!(
            parse_test_targets(MANIFEST),
            vec![
                (
                    "check_queue".to_string(),
                    "tests/checks/check_queue.rs".to_string()
                ),
                (
                    "check_player".to_string(),
                    "tests/check_player.rs".to_string()
                ),
                ("other".to_string(), "tests/other.rs".to_string()),
            ]
        );
    }

    #[test]
    fn reporte_real_de_cargo_da_ejecutable_del_target() {
        let text = include_str!("../fixtures/cargo-test-message.jsonl");
        let report = parse_report(text);
        assert_eq!(report.finished, Some(true));
        let artifact = report
            .artifacts
            .iter()
            .find(|artifact| artifact.target == "check_db_migrations")
            .expect("artifact");
        let executable = artifact.executable.as_ref().expect("executable");
        assert!(executable.ends_with("check_db_migrations-93ad3521c1921555"));
    }

    #[test]
    fn reporte_real_de_error_atribuye_al_target() {
        let text = include_str!("../fixtures/cargo-error-message.jsonl");
        let report = parse_report(text);
        assert!(!report.diagnostics.is_empty());
        assert!(report
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.target.as_deref() == Some("errcrate")));
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rendered.contains("mismatched types")));
        assert!(report.artifacts.is_empty());
    }

    #[test]
    fn descubrimiento_usa_manifiestos_del_repo() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let inventory = discover(root);
        let ui = inventory.get("ui_player").expect("ui_player");
        assert_eq!(ui[0].package, "madrigal-ui");
        assert_eq!(ui[0].name, "check_ui_player");
        assert!(ui[0].dir.join("tests/checks/check_ui_player.rs").is_file());
        let artwork = inventory.get("artwork").expect("artwork");
        assert_eq!(artwork.len(), 2);
    }
}
