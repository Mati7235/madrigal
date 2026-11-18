use std::collections::HashMap;
use std::future::Future;

use futures_lite::StreamExt as _;

use zbus::{MessageStream, connection};
use zbus::{MatchRule, message::Type};
use zbus::zvariant::Value;

use crate::builder::{APP_NAME, BUS_NAME, INTERFACE, OBJECT_PATH, NotificationBuilder};

#[derive(Debug, Clone)]
pub struct NotifyRequest {
    pub app_name: String,
    pub replaces_id: u32,
    pub icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub image_path: Option<String>,
    pub expire_timeout_ms: i32,
}

impl NotifyRequest {
    pub fn from_builder(builder: &NotificationBuilder) -> Self {
        Self {
            app_name: APP_NAME.to_owned(),
            replaces_id: builder.replaces_id,
            icon: builder.icon.clone(),
            summary: builder.summary.clone(),
            body: builder.body.clone(),
            actions: builder.flat_actions(),
            image_path: unless_empty(&builder.icon),
            expire_timeout_ms: builder.expire_timeout_ms,
        }
    }
}

fn unless_empty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

#[derive(Debug)]
pub enum NotifyError {
    Dbus(String),
    Closed,
}

impl std::fmt::Display for NotifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotifyError::Dbus(detail) => write!(f, "dbus: {detail}"),
            NotifyError::Closed => write!(f, "action stream closed"),
        }
    }
}

impl From<zbus::Error> for NotifyError {
    fn from(error: zbus::Error) -> Self {
        NotifyError::Dbus(error.to_string())
    }
}

impl From<zbus::fdo::Error> for NotifyError {
    fn from(error: zbus::fdo::Error) -> Self {
        NotifyError::Dbus(error.to_string())
    }
}

pub trait NotificationBackend: Send + Sync {
    fn notify(
        &self,
        request: &NotifyRequest,
    ) -> impl Future<Output = Result<u32, NotifyError>> + Send;
    fn server_capabilities(
        &self,
    ) -> impl Future<Output = Result<Vec<String>, NotifyError>> + Send;
    fn next_action(&self) -> impl Future<Output = Result<(u32, String), NotifyError>> + Send;
}

pub struct ZbusBackend {
    conn: connection::Connection,
}

impl ZbusBackend {
    pub async fn connect(bus_address: Option<&str>) -> Result<Self, NotifyError> {
        let builder = match bus_address {
            Some(address) => connection::Builder::address(address)?,
            None => connection::Builder::session()?,
        };
        Ok(Self {
            conn: builder.build().await?,
        })
    }

    pub fn connection(&self) -> &connection::Connection {
        &self.conn
    }

    pub async fn subscribe_actions(&self) -> Result<ActionSubscription, NotifyError> {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .interface(INTERFACE)?
            .member("ActionInvoked")?
            .build();
        let stream = MessageStream::for_match_rule(rule, &self.conn, None).await?;
        Ok(ActionSubscription { stream })
    }
}

impl NotificationBackend for ZbusBackend {
    async fn notify(&self, request: &NotifyRequest) -> Result<u32, NotifyError> {
        let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
        if let Some(image) = request.image_path.as_deref() {
            hints.insert("image-path", Value::from(image));
        }
        let reply = self
            .conn
            .call_method(
                Some(BUS_NAME),
                OBJECT_PATH,
                Some(INTERFACE),
                "Notify",
                &(
                    request.app_name.as_str(),
                    request.replaces_id,
                    request.icon.as_str(),
                    request.summary.as_str(),
                    request.body.as_str(),
                    request.actions.clone(),
                    hints,
                    request.expire_timeout_ms,
                ),
            )
            .await?;
        Ok(reply.body().deserialize::<u32>()?)
    }

    async fn server_capabilities(&self) -> Result<Vec<String>, NotifyError> {
        let reply = self
            .conn
            .call_method(
                Some(BUS_NAME),
                OBJECT_PATH,
                Some(INTERFACE),
                "GetCapabilities",
                &(),
            )
            .await?;
        Ok(reply.body().deserialize::<Vec<String>>()?)
    }

    async fn next_action(&self) -> Result<(u32, String), NotifyError> {
        self.subscribe_actions().await?.next().await
    }
}

pub struct ActionSubscription {
    stream: MessageStream,
}

impl ActionSubscription {
    pub async fn next(&mut self) -> Result<(u32, String), NotifyError> {
        while let Some(message) = self.stream.next().await {
            let message = message?;
            if let Ok(action) = message.body().deserialize::<(u32, String)>() {
                return Ok(action);
            }
        }
        Err(NotifyError::Closed)
    }
}

pub struct Notifier<B> {
    backend: B,
}

impl<B: NotificationBackend> Notifier<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub async fn show(&self, builder: &NotificationBuilder) -> Result<u32, NotifyError> {
        self.backend.notify(&NotifyRequest::from_builder(builder)).await
    }

    pub async fn server_capabilities(&self) -> Result<Vec<String>, NotifyError> {
        self.backend.server_capabilities().await
    }

    pub async fn next_action(&self) -> Result<(u32, String), NotifyError> {
        self.backend.next_action().await
    }
}
