use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = "madrigal";
const APPIMAGE: &str = "Madrigal-x86_64.AppImage";
const ARCH: &str = "x86_64";
const APP_RUN: &str = r#"#!/bin/sh
APPDIR="${APPDIR:-$(dirname "$(readlink -f "$0")")}"
if [ -z "${MADRIGAL_YTDLP:-}" ] && [ -x "$APPDIR/usr/bin/yt-dlp" ]; then
    export MADRIGAL_YTDLP="$APPDIR/usr/bin/yt-dlp"
fi
export LD_LIBRARY_PATH="$APPDIR/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$APPDIR/usr/bin/madrigal" "$@"
"#;

pub fn run() -> Result<(), String> {
    let root = workspace_root()?;
    let target = target_dir(&root, std::env::var_os("CARGO_TARGET_DIR"));
    build(&root)?;
    let binary = target.join("release").join(BIN);
    if !binary.is_file() {
        return Err(format!("no se encontró el binario: {}", binary.display()));
    }
    let ytdlp = ytdlp_path(std::env::var_os("YTDLP"))?;
    let appdir = stage(&root, &target, &binary, ytdlp.as_deref())?;
    deploy(&appdir, &target.join("appimage").join("AppRun"))?;
    ensure_icon(&appdir)?;
    let output = package(&target, &appdir)?;
    let size = fs::metadata(&output)
        .map_err(|err| format!("no se pudo leer {}: {err}", output.display()))?
        .len();
    println!(
        "appimage: {} ({} bytes, {:.1} MiB)",
        output.display(),
        size,
        size as f64 / (1024.0 * 1024.0)
    );
    println!("librerías empaquetadas: {}", bundled_libs(&appdir));
    Ok(())
}

fn workspace_root() -> Result<PathBuf, String> {
    let mut dir = std::env::current_dir().map_err(|err| format!("cwd: {err}"))?;
    loop {
        let manifest = dir.join("Cargo.toml");
        if fs::read_to_string(&manifest).is_ok_and(|text| text.contains("[workspace]")) {
            return Ok(dir);
        }
        dir = dir
            .parent()
            .ok_or("no se encontró el Cargo.toml del workspace")?
            .to_path_buf();
    }
}

fn target_dir(root: &Path, env: Option<OsString>) -> PathBuf {
    match env.filter(|value| !value.is_empty()) {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            if dir.is_absolute() {
                dir
            } else {
                root.join(dir)
            }
        }
        None => root.join("target"),
    }
}

fn ytdlp_path(env: Option<OsString>) -> Result<Option<PathBuf>, String> {
    let Some(value) = env.filter(|value| !value.is_empty()) else {
        println!("nota: YTDLP sin definir; yt-dlp no se empaqueta");
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if path.is_file() {
        Ok(Some(path))
    } else {
        Err(format!("YTDLP no es un archivo: {}", path.display()))
    }
}

fn build(root: &Path) -> Result<(), String> {
    let status = crate::cargo_command()
        .current_dir(root)
        .args(["build", "--release", "-p", "madrigal-app", "--bin", BIN])
        .status()
        .map_err(|err| format!("no se pudo ejecutar cargo: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo build falló ({status})"))
    }
}

fn stage(
    root: &Path,
    target: &Path,
    binary: &Path,
    ytdlp: Option<&Path>,
) -> Result<PathBuf, String> {
    let appdir = target.join("appimage").join("Madrigal.AppDir");
    if appdir.exists() {
        fs::remove_dir_all(&appdir)
            .map_err(|err| format!("limpiar {}: {err}", appdir.display()))?;
    }
    let bin_dir = appdir.join("usr/bin");
    let apps_dir = appdir.join("usr/share/applications");
    let icons_dir = appdir.join("usr/share/icons/hicolor/scalable/apps");
    for dir in [&bin_dir, &apps_dir, &icons_dir] {
        fs::create_dir_all(dir).map_err(|err| format!("crear {}: {err}", dir.display()))?;
    }
    copy(binary, &bin_dir.join(BIN))?;
    if let Some(ytdlp) = ytdlp {
        copy(ytdlp, &bin_dir.join("yt-dlp"))?;
    }
    copy(
        &root.join("packaging/madrigal.desktop"),
        &apps_dir.join("madrigal.desktop"),
    )?;
    copy(
        &root.join("packaging/madrigal.svg"),
        &icons_dir.join("madrigal.svg"),
    )?;
    let apprun = target.join("appimage").join("AppRun");
    fs::write(&apprun, APP_RUN).map_err(|err| format!("escribir {}: {err}", apprun.display()))?;
    fs::set_permissions(&apprun, fs::Permissions::from_mode(0o755))
        .map_err(|err| format!("chmod {}: {err}", apprun.display()))?;
    Ok(appdir)
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|err| format!("copiar {} -> {}: {err}", from.display(), to.display()))
}

fn tool(env: &str, default: &str) -> OsString {
    std::env::var_os(env)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| OsString::from(default))
}

fn run_tool(label: &str, program: &OsString, args: &[OsString]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .env("APPIMAGE_EXTRACT_AND_RUN", "1")
        .env("ARCH", ARCH)
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("LD_PRELOAD")
        .env_remove("LIBRARY_PATH")
        .status()
        .map_err(|err| {
            format!(
                "{label}: no se pudo ejecutar {}: {err}",
                Path::new(program).display()
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} terminó con {status}"))
    }
}

fn deploy(appdir: &Path, apprun: &Path) -> Result<(), String> {
    let args: Vec<OsString> = [
        "--appdir",
        appdir.to_str().ok_or("ruta del AppDir no UTF-8")?,
        "--executable",
        appdir
            .join("usr/bin/madrigal")
            .to_str()
            .ok_or("ruta no UTF-8")?,
        "--desktop-file",
        appdir
            .join("usr/share/applications/madrigal.desktop")
            .to_str()
            .ok_or("ruta no UTF-8")?,
        "--icon-file",
        appdir
            .join("usr/share/icons/hicolor/scalable/apps/madrigal.svg")
            .to_str()
            .ok_or("ruta no UTF-8")?,
        "--custom-apprun",
        apprun.to_str().ok_or("ruta no UTF-8")?,
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    run_tool("linuxdeploy", &tool("LINUXDEPLOY", "linuxdeploy"), &args)?;
    let mpv = appdir.join("usr/lib/libmpv.so.2");
    if !lib_exists(&mpv) {
        return Err(format!(
            "linuxdeploy no empaquetó libmpv: {}",
            mpv.display()
        ));
    }
    let installed = appdir.join("AppRun");
    if !installed.is_file() {
        return Err(format!("linuxdeploy no instaló {}", installed.display()));
    }
    fs::set_permissions(&installed, fs::Permissions::from_mode(0o755))
        .map_err(|err| format!("chmod {}: {err}", installed.display()))?;
    Ok(())
}

fn lib_exists(path: &Path) -> bool {
    fs::metadata(path).is_ok()
}

fn ensure_icon(appdir: &Path) -> Result<(), String> {
    let root_icon = appdir.join("madrigal.svg");
    if fs::symlink_metadata(&root_icon).is_err() {
        symlink(
            "usr/share/icons/hicolor/scalable/apps/madrigal.svg",
            &root_icon,
        )
        .map_err(|err| format!("enlazar {}: {err}", root_icon.display()))?;
    }
    let dir_icon = appdir.join(".DirIcon");
    if fs::symlink_metadata(&dir_icon).is_err() {
        symlink("madrigal.svg", &dir_icon)
            .map_err(|err| format!("enlazar {}: {err}", dir_icon.display()))?;
    }
    Ok(())
}

fn package(target: &Path, appdir: &Path) -> Result<PathBuf, String> {
    let output = target.join("appimage").join(APPIMAGE);
    if output.exists() {
        fs::remove_file(&output).map_err(|err| format!("borrar {}: {err}", output.display()))?;
    }
    let args: Vec<OsString> = [
        appdir.to_str().ok_or("ruta del AppDir no UTF-8")?,
        output.to_str().ok_or("ruta de salida no UTF-8")?,
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    run_tool("appimagetool", &tool("APPIMAGETOOL", "appimagetool"), &args)?;
    if !output.is_file() {
        return Err(format!("appimagetool no generó {}", output.display()));
    }
    Ok(output)
}

fn bundled_libs(appdir: &Path) -> usize {
    fn walk(dir: &Path, count: &mut usize) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, count);
            } else if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".so"))
            {
                *count += 1;
            }
        }
    }
    let mut count = 0;
    walk(&appdir.join("usr/lib"), &mut count);
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_dir_respeta_la_variable_y_el_default() {
        let root = Path::new("/repo");
        assert_eq!(
            target_dir(root, Some(OsString::from("/mnt/build/target"))),
            PathBuf::from("/mnt/build/target")
        );
        assert_eq!(
            target_dir(root, Some(OsString::from("out"))),
            PathBuf::from("/repo/out")
        );
        assert_eq!(
            target_dir(root, Some(OsString::new())),
            PathBuf::from("/repo/target")
        );
        assert_eq!(target_dir(root, None), PathBuf::from("/repo/target"));
    }

    #[test]
    fn ytdlp_solo_empaqueta_un_archivo_existente() {
        assert_eq!(ytdlp_path(None).unwrap(), None);
        assert_eq!(ytdlp_path(Some(OsString::new())).unwrap(), None);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let icon = root.join("packaging/madrigal.svg");
        assert_eq!(
            ytdlp_path(Some(icon.clone().into_os_string())).unwrap(),
            Some(icon)
        );
        assert!(ytdlp_path(Some(OsString::from("/no/existe/yt-dlp"))).is_err());
    }

    #[test]
    fn apprun_solo_exporta_yt_dlp_si_el_usuario_no_lo_definio() {
        assert!(APP_RUN.contains(r#"if [ -z "${MADRIGAL_YTDLP:-}" ]"#));
        assert!(APP_RUN.contains(r#"export MADRIGAL_YTDLP="$APPDIR/usr/bin/yt-dlp""#));
        assert!(APP_RUN.contains(r#"exec "$APPDIR/usr/bin/madrigal" "$@""#));
        assert!(APP_RUN.contains(r#"LD_LIBRARY_PATH="$APPDIR/usr/lib"#));
    }

    #[test]
    fn empaqueta_desktop_e_icono_validos() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let desktop = fs::read_to_string(root.join("packaging/madrigal.desktop")).unwrap();
        for line in [
            "Name=Madrigal",
            "Exec=madrigal %F",
            "Icon=madrigal",
            "Terminal=false",
            "Categories=AudioVideo;Audio;Player;",
        ] {
            assert!(desktop.lines().any(|entry| entry == line), "{line}");
        }
        assert!(desktop
            .lines()
            .any(|entry| entry.starts_with("MimeType=audio/mpeg;")));
        let icon = fs::read_to_string(root.join("packaging/madrigal.svg")).unwrap();
        assert!(icon.starts_with("<svg"));
        assert!(icon.contains("viewBox=\"0 0 512 512\""));
        assert!(icon.trim_end().ends_with("</svg>"));
    }
}
