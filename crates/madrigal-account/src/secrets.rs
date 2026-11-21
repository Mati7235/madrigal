use std::collections::HashMap;

const KWALLET_SERVICES: [&str; 3] = ["org.kde.kwalletd6", "org.kde.kwalletd5", "org.kde.kwalletd"];
const KWALLET_FOLDERS: [&str; 2] = ["Chrome Keys", "Chromium Keys"];
const KWALLET_KEYS: [&str; 6] = [
    "Chrome Safe Storage",
    "Chromium Safe Storage",
    "Brave Safe Storage",
    "Microsoft Edge Safe Storage",
    "Vivaldi Safe Storage",
    "Opera Safe Storage",
];

fn from_secret_service(app_names: &[&str], labels: &[&str]) -> Option<String> {
    let service =
        secret_service::blocking::SecretService::connect(secret_service::EncryptionType::Plain)
            .ok()?;
    for app_name in app_names {
        let Ok(found) = service.search_items(HashMap::from([("application", *app_name)])) else {
            continue;
        };
        for item in found.unlocked {
            let Ok(label) = item.get_label() else {
                continue;
            };
            if !labels.contains(&label.as_str()) {
                continue;
            }
            let Ok(secret) = item.get_secret() else {
                continue;
            };
            let Ok(password) = String::from_utf8(secret) else {
                continue;
            };
            if !password.is_empty() {
                return Some(password);
            }
        }
    }
    let Ok(collections) = service.get_all_collections() else {
        return None;
    };
    for collection in collections {
        let Ok(items) = collection.get_all_items() else {
            continue;
        };
        for item in items {
            let Ok(label) = item.get_label() else {
                continue;
            };
            if !labels.contains(&label.as_str()) {
                continue;
            }
            let Ok(secret) = item.get_secret() else {
                continue;
            };
            let Ok(password) = String::from_utf8(secret) else {
                continue;
            };
            if !password.is_empty() {
                return Some(password);
            }
        }
    }
    None
}

fn from_kwallet(product: &str, labels: &[&str]) -> Option<String> {
    let connection = zbus::blocking::Connection::session().ok()?;
    for service in KWALLET_SERVICES {
        let Some(name) = service.rsplit('.').next() else {
            continue;
        };
        let path = format!("/modules/{name}");
        let Ok(proxy) =
            zbus::blocking::Proxy::new(&connection, service, path.as_str(), "org.kde.KWallet")
        else {
            continue;
        };
        let enabled: bool = match proxy.call("isEnabled", &()) {
            Ok(enabled) => enabled,
            Err(_) => continue,
        };
        if !enabled {
            continue;
        }
        let wallet: String = match proxy.call("networkWallet", &()) {
            Ok(wallet) => wallet,
            Err(_) => continue,
        };
        for app_id in [product, labels[0]] {
            let Ok(handle) = proxy.call::<_, _, i32>("open", &(wallet.as_str(), 0i64, app_id))
            else {
                continue;
            };
            if handle < 0 {
                continue;
            }
            for folder in KWALLET_FOLDERS.iter().copied().chain(std::iter::once(product)) {
                for key in KWALLET_KEYS {
                    let has: bool = proxy
                        .call("hasEntry", &(handle, folder, key))
                        .unwrap_or(false);
                    if !has {
                        continue;
                    }
                    let password: String = proxy
                        .call("readPassword", &(handle, folder, key, app_id))
                        .unwrap_or_default();
                    if !password.is_empty() {
                        return Some(password);
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn safe_storage_password(
    app_names: &[&str],
    labels: &[&str],
    product: &str,
) -> Option<String> {
    if labels.is_empty() {
        return None;
    }
    from_secret_service(app_names, labels).or_else(|| from_kwallet(product, labels))
}
