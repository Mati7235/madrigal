use madrigal_i18n::{DEFAULT_LANG, LANGS, Lang, contains, format, keys, len, tr, trf, try_tr};
use std::collections::HashSet;

#[test]
fn idioma_por_defecto_es_y_disponibles_es_en() {
    assert_eq!(DEFAULT_LANG, Lang::Es);
    assert_eq!(LANGS, &[Lang::Es, Lang::En]);
}

#[test]
fn tabla_ordenada_sin_duplicadas() {
    let all: Vec<&str> = keys().collect();
    assert!(!all.is_empty());
    assert_eq!(len(), all.len());
    let mut sorted = all.clone();
    sorted.sort_unstable();
    assert_eq!(all, sorted);
    let unique: HashSet<&str> = all.into_iter().collect();
    assert_eq!(len(), unique.len());
}

#[test]
fn paridad_es_en_y_sin_huerfanas() {
    for key in keys() {
        let es = try_tr(Lang::Es, key);
        let en = try_tr(Lang::En, key);
        assert!(es.is_ok(), "{key} falta en es");
        assert!(en.is_ok(), "{key} falta en en");
        assert!(!es.unwrap().is_empty(), "{key} vacia en es");
        assert!(!en.unwrap().is_empty(), "{key} vacia en en");
        assert!(contains(key), "{key} no visible en contains");
    }
}

#[test]
fn tr_devuelve_no_vacio_en_ambos_idiomas() {
    for key in keys() {
        for lang in LANGS {
            assert!(!tr(*lang, key).is_empty(), "{key} vacia en {lang:?}");
        }
    }
}

#[test]
fn formato_sustituye_argumentos_en_orden() {
    assert_eq!(format("Tono: %1 semitonos", &["2"]), "Tono: 2 semitonos");
    assert_eq!(format("Velocidad: %1x", &["1.25"]), "Velocidad: 1.25x");
    assert_eq!(format("%1 y %2", &["a", "b"]), "a y b");
    assert_eq!(format("100% %1", &["ok"]), "100% ok");
    assert_eq!(format("%1 %2", &["solo"]), "solo %2");
    assert_eq!(format("%0 %10", &["x"]), "%0 %10");
    assert_eq!(format("sin argumentos", &[]), "sin argumentos");
}

#[test]
fn claves_con_marcadores_se_formatean_sin_dejarlos() {
    for lang in LANGS {
        for key in keys() {
            let template = tr(*lang, key);
            if !template.contains('%') {
                continue;
            }
            let filled = format(template, &["1", "2"]);
            if template.contains("%1") || template.contains("%2") {
                assert!(!filled.contains("%1") && !filled.contains("%2"), "{key} sin formatear");
            }
        }
    }
    assert_eq!(
        trf(Lang::Es, "audio.speed", &["1.5"]),
        "Velocidad: 1.5×"
    );
    assert_eq!(trf(Lang::En, "audio.pitch", &["-2"]), "Pitch: -2 semitones");
}

#[test]
fn clave_inexistente_da_error_testeable() {
    for lang in LANGS {
        let err = try_tr(*lang, "no.existe.esta.clave").unwrap_err();
        assert_eq!(err.0, "no.existe.esta.clave");
        assert!(err.to_string().contains("no.existe.esta.clave"));
        assert!(!contains("no.existe.esta.clave"));
        assert_eq!(tr(*lang, "no.existe.esta.clave"), "no.existe.esta.clave");
    }
}
