use madrigal_account::auth::{browser_auth, on_behalf_of_user, USER_AGENT};
use madrigal_account::cookies::{sapisidhash, YOUTUBE_MUSIC_ORIGIN};
use madrigal_account::session::Session;

const FIXED: i64 = 1_700_000_000;
const GOLDEN: &str = "SAPISIDHASH 1700000000_1a84dc71f1aa1aec2e81f410065431f4dde10153";
const FIRST_WINS: &str = "SAPISIDHASH 1700000000_2a6244abbf43e7006c9e666adeab924fb7f03efc";
const FALLBACK_1P: &str = "SAPISIDHASH 1700000000_f217da8ca1a11a2c3092f7bb8d4fcc944edd6ad7";
const LOGIN_COOKIE: &str =
    "LOGIN_INFO=abc:def; __Secure-3PAPISID=AbCdEf123; SAPISID=primero; PREF=hl=es";
const GOLDEN_COOKIE: &str = "LOGIN_INFO=abc:def; __Secure-3PAPISID=AbCdEf123";

fn full_session() -> Session {
    Session {
        cookie: GOLDEN_COOKIE.to_owned(),
        visitor_data: "CgtWSVNJVE9SREFUQQ%3D%3D".to_owned(),
        data_sync_id: "1234567890".to_owned(),
        account_name: "Cuenta de prueba".to_owned(),
        ..Session::default()
    }
}

#[test]
fn account_sapisid() {
    assert_eq!(
        sapisidhash(GOLDEN_COOKIE, YOUTUBE_MUSIC_ORIGIN, FIXED).as_deref(),
        Some(GOLDEN)
    );
    assert_eq!(
        sapisidhash("PREF=hl=es", YOUTUBE_MUSIC_ORIGIN, FIXED),
        None
    );
    assert_eq!(
        sapisidhash(LOGIN_COOKIE, YOUTUBE_MUSIC_ORIGIN, FIXED).as_deref(),
        Some(FIRST_WINS)
    );
    assert_eq!(
        sapisidhash(
            "__Secure-1PAPISID=otro; LOGIN_INFO=x",
            YOUTUBE_MUSIC_ORIGIN,
            FIXED
        )
        .as_deref(),
        Some(FALLBACK_1P)
    );
    assert_ne!(
        sapisidhash(GOLDEN_COOKIE, "https://www.youtube.com", FIXED).as_deref(),
        Some(GOLDEN)
    );
    assert_ne!(
        sapisidhash(GOLDEN_COOKIE, YOUTUBE_MUSIC_ORIGIN, FIXED + 1).as_deref(),
        Some(GOLDEN)
    );

    assert_eq!(browser_auth(&Session::default(), FIXED), None);
    assert_eq!(
        browser_auth(
            &Session {
                cookie: "LOGIN_INFO=a; SAPISID=b".to_owned(),
                ..Session::default()
            },
            FIXED
        ),
        None
    );

    let headers = browser_auth(&full_session(), FIXED).expect("headers");
    assert_eq!(headers.cookie, GOLDEN_COOKIE);
    assert_eq!(headers.authorization, GOLDEN);
    assert_eq!(headers.authuser, "0");
    assert_eq!(headers.origin, YOUTUBE_MUSIC_ORIGIN);
    assert_eq!(headers.user_agent, USER_AGENT);
    assert_eq!(
        headers.visitor_id.as_deref(),
        Some("CgtWSVNJVE9SREFUQQ%3D%3D")
    );
    let pairs = headers.pairs();
    assert_eq!(pairs.len(), 6);
    assert!(pairs.contains(&("authorization".to_owned(), GOLDEN.to_owned())));

    let channel = Session {
        authuser: "2".to_owned(),
        visitor_data: String::new(),
        ..full_session()
    };
    let branded = browser_auth(&channel, FIXED).expect("headers");
    assert_eq!(branded.authuser, "2");
    assert_eq!(branded.visitor_id, None);
    assert_eq!(branded.pairs().len(), 5);

    assert_eq!(on_behalf_of_user(&full_session()), Some("1234567890"));
    assert_eq!(on_behalf_of_user(&Session::default()), None);
}
