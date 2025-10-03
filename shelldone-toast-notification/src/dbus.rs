#![cfg(all(not(target_os = "macos"), not(windows)))]
//! See <https://developer.gnome.org/notification-spec/>

use crate::ToastNotification;
use futures_util::stream::{abortable, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::proxy::SignalStream;
use zbus::Proxy;
use zvariant::{Type, Value};

#[derive(Debug, Type, Serialize, Deserialize)]
pub struct ServerInformation {
    /// The product name of the server.
    pub name: String,

    /// The vendor name. For example "KDE," "GNOME," "freedesktop.org" or "Microsoft".
    pub vendor: String,

    /// The server's version number.
    pub version: String,

    /// The specification version the server is compliant with.
    pub spec_version: String,
}

type NotificationBody<'a, 'h> = (
    &'a str,
    u32,
    &'a str,
    &'a str,
    &'a str,
    &'a [&'a str],
    &'h HashMap<&'h str, Value<'h>>,
    i32,
);

struct NotificationPayload<'a, 'h> {
    app_name: &'a str,
    replaces_id: u32,
    app_icon: &'a str,
    summary: &'a str,
    body: &'a str,
    actions: &'a [&'a str],
    hints: &'h HashMap<&'h str, Value<'h>>,
    expire_timeout: i32,
}

impl<'a, 'h> NotificationPayload<'a, 'h> {
    fn as_tuple(&'a self) -> NotificationBody<'a, 'h> {
        (
            self.app_name,
            self.replaces_id,
            self.app_icon,
            self.summary,
            self.body,
            self.actions,
            self.hints,
            self.expire_timeout,
        )
    }
}

struct NotificationsProxy<'a> {
    inner: Proxy<'a>,
}

impl<'a> NotificationsProxy<'a> {
    async fn new(connection: &'a zbus::Connection) -> zbus::Result<Self> {
        let inner = Proxy::new(
            connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )
        .await?;
        Ok(Self { inner })
    }

    async fn get_server_information(&self) -> zbus::Result<ServerInformation> {
        self.inner.call("GetServerInformation", &()).await
    }

    async fn get_capabilities(&self) -> zbus::Result<Vec<String>> {
        self.inner.call("GetCapabilities", &()).await
    }

    async fn notify(&self, payload: &NotificationPayload<'_, '_>) -> zbus::Result<u32> {
        let body = payload.as_tuple();
        self.inner.call("Notify", &body).await
    }

    async fn receive_action_invoked(&self) -> zbus::Result<SignalStream<'_>> {
        self.inner.receive_signal("ActionInvoked").await
    }

    async fn receive_notification_closed(&self) -> zbus::Result<SignalStream<'_>> {
        self.inner.receive_signal("NotificationClosed").await
    }
}

/// Timeout/expiration was reached
const REASON_EXPIRED: u32 = 1;
/// User dismissed it
const REASON_USER_DISMISSED: u32 = 2;
/// CloseNotification was called with the nid
const REASON_CLOSE_NOTIFICATION: u32 = 3;

#[derive(Debug)]
enum Reason {
    Expired,
    Dismissed,
    Closed,
    #[allow(dead_code)]
    Unknown(u32),
}

impl Reason {
    fn new(n: u32) -> Self {
        match n {
            REASON_EXPIRED => Self::Expired,
            REASON_USER_DISMISSED => Self::Dismissed,
            REASON_CLOSE_NOTIFICATION => Self::Closed,
            _ => Self::Unknown(n),
        }
    }
}

async fn show_notif_impl(notif: ToastNotification) -> Result<(), Box<dyn std::error::Error>> {
    let connection = zbus::ConnectionBuilder::session()?.build().await?;

    let proxy = NotificationsProxy::new(&connection).await?;
    if let Ok(info) = proxy.get_server_information().await {
        log::trace!(
            "notification server: {} {} v{} (spec {})",
            info.vendor,
            info.name,
            info.version,
            info.spec_version
        );
    }
    let caps = proxy.get_capabilities().await?;

    if notif.url.is_some() && !caps.iter().any(|cap| cap == "actions") {
        // Server doesn't support actions, so skip showing this notification
        // because it might have text that says "click to see more"
        // and that just wouldn't work.
        return Ok(());
    }

    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("urgency", Value::U8(2 /* Critical */));
    let actions: &[&str] = if notif.url.is_some() {
        &["show", "Show"]
    } else {
        &[]
    };
    let notification = proxy
        .notify(&NotificationPayload {
            app_name: "shelldone",
            replaces_id: 0,
            app_icon: "net.shelldone.terminal",
            summary: &notif.title,
            body: &notif.message,
            actions,
            hints: &hints,
            expire_timeout: notif.timeout.map(|d| d.as_millis() as _).unwrap_or(0),
        })
        .await?;

    let (mut invoked_stream, abort_invoked) = abortable(proxy.receive_action_invoked().await?);
    let (mut closed_stream, abort_closed) = abortable(proxy.receive_notification_closed().await?);

    futures_util::try_join!(
        async {
            while let Some(message) = invoked_stream.next().await {
                let (nid, _action_key): (u32, String) = message.body().deserialize()?;
                if nid == notification {
                    if let Some(url) = notif.url.as_ref() {
                        shelldone_open_url::open_url(url);
                        abort_closed.abort();
                        break;
                    }
                }
            }
            Ok::<(), zbus::Error>(())
        },
        async {
            while let Some(message) = closed_stream.next().await {
                let (nid, reason): (u32, u32) = message.body().deserialize()?;
                let _reason = Reason::new(reason);
                if nid == notification {
                    abort_invoked.abort();
                    break;
                }
            }
            Ok::<(), zbus::Error>(())
        }
    )?;

    Ok(())
}

pub fn show_notif(notif: ToastNotification) -> Result<(), Box<dyn std::error::Error>> {
    // Run this in a separate thread as we don't know if dbus or the notification
    // service on the other end are up, and we'd otherwise block for some time.
    std::thread::spawn(move || {
        let res = async_io::block_on(async move { show_notif_impl(notif).await });
        if let Err(err) = res {
            log::error!("while showing notification: {:#}", err);
        }
    });
    Ok(())
}
