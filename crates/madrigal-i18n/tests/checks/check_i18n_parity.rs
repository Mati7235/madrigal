use madrigal_i18n::{DEFAULT_LANG, LANGS, Lang, contains, keys, len, tr, try_tr};
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
fn clave_inexistente_da_error_testeable() {
    for lang in LANGS {
        let err = try_tr(*lang, "no.existe.esta.clave").unwrap_err();
        assert_eq!(err.0, "no.existe.esta.clave");
        assert!(err.to_string().contains("no.existe.esta.clave"));
        assert!(!contains("no.existe.esta.clave"));
        assert_eq!(tr(*lang, "no.existe.esta.clave"), "no.existe.esta.clave");
    }
}
