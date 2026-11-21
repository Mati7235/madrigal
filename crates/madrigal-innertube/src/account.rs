use std::borrow::Cow;

use serde_json::Value;
use ytmapi_rs::auth::AuthToken;
use ytmapi_rs::parse::{ParseFrom, ProcessedResult};
use ytmapi_rs::query::{PostMethod, PostQuery, Query};

use crate::error::Error;
use crate::library::AuthCatalogClient;

#[derive(Clone)]
pub struct GetAccountMenuQuery;

impl<A: AuthToken> Query<A> for GetAccountMenuQuery {
    type Output = AccountMenu;
    type Method = PostMethod;
}

impl PostQuery for GetAccountMenuQuery {
    fn header(&self) -> serde_json::Map<String, Value> {
        serde_json::Map::new()
    }

    fn params(&self) -> Vec<(&str, Cow<'_, str>)> {
        Vec::new()
    }

    fn path(&self) -> &str {
        "account/account_menu"
    }
}

#[derive(Debug)]
pub struct AccountMenu(Value);

impl ParseFrom<GetAccountMenuQuery> for AccountMenu {
    fn parse_from(p: ProcessedResult<GetAccountMenuQuery>) -> ytmapi_rs::Result<Self> {
        Ok(Self(p.json.into_inner()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountInfo {
    pub name: String,
    pub handle: Option<String>,
    pub email: Option<String>,
    pub channel_id: Option<String>,
    pub avatar: Option<String>,
}

impl AuthCatalogClient {
    pub async fn account_info(&self) -> Result<AccountInfo, Error> {
        let menu = self.yt.query(GetAccountMenuQuery).await?;
        account_info_from_value(&menu.0)
    }

    pub fn parse_account_info(source: &str) -> Result<AccountInfo, Error> {
        let value: Value = serde_json::from_str(source)?;
        account_info_from_value(&value)
    }
}

fn account_info_from_value(value: &Value) -> Result<AccountInfo, Error> {
    let header = find_key(value, "activeAccountHeaderRenderer").ok_or(Error::NotSignedIn)?;
    let name = runs_text(header.get("accountName"))
        .ok_or_else(|| Error::Fixture("account menu has no account name".to_owned()))?;
    Ok(AccountInfo {
        name,
        handle: runs_text(header.get("channelHandle")),
        email: runs_text(header.get("email")),
        channel_id: channel_id(value, header),
        avatar: header.get("accountPhoto").and_then(thumbnail_url),
    })
}

fn channel_id(menu: &Value, header: &Value) -> Option<String> {
    your_channel_id(menu).or_else(|| {
        header
            .get("navigationEndpoint")
            .and_then(|endpoint| endpoint.pointer("/browseEndpoint/browseId"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

fn your_channel_id(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            let is_your_channel =
                runs_text(map.get("title")).as_deref() == Some("Your channel");
            if is_your_channel {
                if let Some(id) = map
                    .get("navigationEndpoint")
                    .and_then(|endpoint| endpoint.pointer("/browseEndpoint/browseId"))
                    .and_then(Value::as_str)
                {
                    return Some(id.to_owned());
                }
            }
            map.values().find_map(your_channel_id)
        }
        Value::Array(items) => items.iter().find_map(your_channel_id),
        _ => None,
    }
}

fn find_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|item| find_key(item, key))),
        Value::Array(items) => items.iter().find_map(|item| find_key(item, key)),
        _ => None,
    }
}

fn runs_text(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(runs) = value.get("runs").and_then(Value::as_array) {
        let text: String = runs
            .iter()
            .filter_map(|run| run.get("text").and_then(Value::as_str))
            .collect();
        return (!text.is_empty()).then_some(text);
    }
    value
        .get("simpleText")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn thumbnail_url(photo: &Value) -> Option<String> {
    photo
        .get("thumbnails")?
        .as_array()?
        .last()?
        .get("url")?
        .as_str()
        .map(str::to_owned)
}
