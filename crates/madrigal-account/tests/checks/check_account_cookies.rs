use madrigal_account::cookies::{
    cookie_header, from_netscape, has_login_cookie, has_primary_sapisid, is_youtube_cookie,
    login_cookie_value, merge_login_cookies, parse_cookie_header, sapisid_expired, sapisid_ttl,
    to_netscape, CookieRecord,
};
use madrigal_account::errors::{session_expired_message, session_expired_status, AccountError};

const HEADER: &str = include_str!("../fixtures/cookie_header.txt");
const NETSCAPE: &str = include_str!("../fixtures/cookies_netscape.txt");
const EXPIRY: i64 = 1_700_086_400;

fn login_pairs() -> Vec<(String, String)> {
    parse_cookie_header(HEADER.trim()).expect("fixture header parses")
}

#[test]
fn account_cookies() {
    assert_eq!(
        parse_cookie_header("A=1; B=2 ; =3; C=4").expect("parse"),
        vec![
            ("A".to_owned(), "1".to_owned()),
            ("B".to_owned(), "2".to_owned()),
            ("C".to_owned(), "4".to_owned()),
        ]
    );
    assert_eq!(
        parse_cookie_header("A=1; A=2").expect("parse"),
        vec![("A".to_owned(), "1".to_owned())]
    );
    assert_eq!(
        parse_cookie_header("A=b=c").expect("parse"),
        vec![("A".to_owned(), "b=c".to_owned())]
    );
    assert!(parse_cookie_header("").expect("parse").is_empty());
    assert!(parse_cookie_header("A=1\r\nB=2").is_err());
    assert!(parse_cookie_header("A=1\tB=2").is_err());
    assert_eq!(
        cookie_header(&[("A".to_owned(), "1".to_owned()), ("B".to_owned(), "2".to_owned())]),
        "A=1; B=2"
    );

    let pairs = login_pairs();
    assert_eq!(login_cookie_value(&pairs), Some("primero"));
    assert_eq!(
        login_cookie_value(
            &parse_cookie_header("__Secure-3PAPISID=AbCdEf123").expect("parse")
        ),
        Some("AbCdEf123")
    );
    assert_eq!(
        login_cookie_value(
            &parse_cookie_header("__Secure-1PAPISID=otro").expect("parse")
        ),
        Some("otro")
    );
    assert_eq!(
        login_cookie_value(&parse_cookie_header("PREF=hl=es").expect("parse")),
        None
    );
    assert!(has_primary_sapisid(&pairs));
    assert!(!has_primary_sapisid(
        &parse_cookie_header("SAPISID=x").expect("parse")
    ));
    assert!(has_login_cookie(&pairs));
    assert!(!has_login_cookie(
        &parse_cookie_header("__Secure-3PAPISID=AbCdEf123").expect("parse")
    ));
    assert!(!has_login_cookie(
        &parse_cookie_header("LOGIN_INFO=; SAPISID=AbCdEf123").expect("parse")
    ));

    for domain in [
        ".youtube.com",
        "youtube.com",
        ".music.youtube.com",
        "music.youtube.com",
        "www.youtube.com",
    ] {
        assert!(is_youtube_cookie(domain), "{domain} is YouTube");
    }
    for domain in ["google.com", "evil-youtube.com", ".youtube.com.evil.io", ""] {
        assert!(!is_youtube_cookie(domain), "{domain} is not YouTube");
    }

    let records = from_netscape(NETSCAPE).expect("fixture parses");
    assert_eq!(records.len(), 4);
    assert_eq!(records[0].name, "LOGIN_INFO");
    assert!(records[0].http_only && records[0].secure);
    assert_eq!(records[1].value, "AbCdEf123");
    assert!(!records[2].secure && !records[2].http_only);
    assert_eq!(records[2].domain, ".youtube.com");
    assert_eq!(records[3].domain, ".example.com");
    assert!(records.iter().all(|record| record.expires == EXPIRY || record.name == "ajena"));
    assert!(from_netscape("a\tb\tc\n").is_err());

    let text = to_netscape(&records).expect("serialize");
    assert!(text.starts_with("# Netscape HTTP Cookie File\n"));
    assert!(text.ends_with('\n'));
    assert!(text.contains("#HttpOnly_.youtube.com\tTRUE\t/\tTRUE"));
    assert!(text.contains("\n.youtube.com\tTRUE\t/\tFALSE"));
    assert_eq!(from_netscape(&text).expect("roundtrip"), records);
    let bad = CookieRecord::new("A\r", "1", ".youtube.com");
    assert!(to_netscape(&[bad]).is_err());

    let youtube: Vec<CookieRecord> = records
        .iter()
        .filter(|record| is_youtube_cookie(&record.domain))
        .cloned()
        .collect();
    assert_eq!(youtube.len(), 3);
    assert_eq!(
        merge_login_cookies(&youtube),
        "LOGIN_INFO=abc:def; __Secure-3PAPISID=AbCdEf123; PREF=hl=es"
    );

    assert!(!records[0].is_expired(EXPIRY - 1));
    assert!(records[0].is_expired(EXPIRY));
    assert!(records[3].is_expired(EXPIRY));
    assert!(!sapisid_expired(&youtube, EXPIRY - 1));
    assert!(sapisid_expired(&youtube, EXPIRY));
    assert_eq!(sapisid_ttl(&youtube, EXPIRY - 3_600), Some(3_600));
    assert_eq!(sapisid_ttl(&youtube, EXPIRY), Some(0));
    let session_bound = vec![CookieRecord::new("SAPISID", "x", ".youtube.com")];
    assert!(!sapisid_expired(&session_bound, i64::MAX));
    assert_eq!(sapisid_ttl(&session_bound, EXPIRY), None);
    assert!(!sapisid_expired(&[], EXPIRY));

    assert!(AccountError::Unauthorized.expired());
    assert!(!AccountError::CookieIncomplete.expired());
    for message in [
        "HTTP Error 401: Unauthorized",
        "Sign in to confirm you're not a bot",
        "LOGIN_REQUIRED",
        "invalid_grant",
        "session expired",
        "accounts.google.com/x",
    ] {
        assert!(session_expired_message(message), "{message} reads expired");
    }
    for message in ["Requested format is not available", "404 Not Found", ""] {
        assert!(!session_expired_message(message), "{message} reads fresh");
    }
    assert!(session_expired_status(401));
    assert!(session_expired_status(403));
    assert!(!session_expired_status(500));
}
