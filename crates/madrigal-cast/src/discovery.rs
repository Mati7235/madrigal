use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::protocol::SERVICE_TYPE;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastDevice {
    pub id: String,
    pub name: String,
    pub model: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRecord {
    pub fullname: String,
    pub host: String,
    pub port: u16,
    pub txt: HashMap<String, String>,
}

impl ServiceRecord {
    pub fn txt_value(&self, key: &str) -> Option<&str> {
        self.txt.get(key).map(String::as_str)
    }
}

pub fn device_from_record(record: &ServiceRecord) -> Option<CastDevice> {
    if record.host.is_empty() || record.port == 0 {
        return None;
    }
    let name = record
        .txt_value("fn")
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| short_name(&record.fullname));
    let id = record
        .txt_value("id")
        .filter(|id| !id.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| record.fullname.clone());
    Some(CastDevice {
        id,
        name,
        model: record.txt_value("md").unwrap_or("").to_owned(),
        host: record.host.clone(),
        port: record.port,
    })
}

fn short_name(fullname: &str) -> String {
    fullname
        .split('.')
        .next()
        .unwrap_or(fullname)
        .replace("\\032", " ")
}

pub fn browse(timeout: Duration) -> Result<Vec<CastDevice>> {
    let daemon = mdns_sd::ServiceDaemon::new().map_err(|error| Error::Mdns(error.to_string()))?;
    let receiver = daemon
        .browse(SERVICE_TYPE)
        .map_err(|error| Error::Mdns(error.to_string()))?;
    let deadline = Instant::now() + timeout;
    let mut devices = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                if let Some(device) = device_from_record(&record_from_info(&info)) {
                    if !devices.iter().any(|known: &CastDevice| known.id == device.id) {
                        devices.push(device);
                    }
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    Ok(devices)
}

fn record_from_info(info: &mdns_sd::ResolvedService) -> ServiceRecord {
    let host = info
        .get_addresses_v4()
        .iter()
        .next()
        .map(ToString::to_string)
        .or_else(|| info.get_addresses().iter().next().map(|ip| ip.to_string()))
        .unwrap_or_default();
    let mut txt = HashMap::new();
    for property in info.get_properties().iter() {
        txt.insert(property.key().to_owned(), property.val_str().to_owned());
    }
    ServiceRecord { fullname: info.get_fullname().to_owned(), host, port: info.get_port(), txt }
}
