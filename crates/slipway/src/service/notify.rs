//! The notification: one per new version, with a button that opens the Software Update page.

use std::collections::HashMap;
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use zbus::export::futures_core::Stream;
use zbus::fdo::DBusProxy;
use zbus::message::Type as MessageType;
use zbus::names::BusName;
use zbus::zvariant::Value;
use zbus::{Connection, MatchRule, MessageStream};

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const APP_NAME: &str = "Software Update";
const DEFAULT_ACTION: &str = "default";
const DETAILS: &str = "Details";

/// The ids of the notifications this daemon posted, so a pressed button can be recognised.
#[derive(Clone, Debug, Default)]
pub struct Posted(Arc<Mutex<Vec<u32>>>);

impl Posted {
    fn remember(&self, id: u32) {
        let mut ids = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        ids.push(id);
        let excess = ids.len().saturating_sub(32);
        ids.drain(..excess);
    }

    fn knows(&self, id: u32) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&id)
    }
}

/// Posts a notification with the "Details" button; returns its id.
pub async fn post(connection: &Connection, posted: &Posted, body: &str) -> zbus::Result<u32> {
    let hints: HashMap<&str, Value<'_>> = HashMap::new();
    let actions = vec![DEFAULT_ACTION, DETAILS];
    let reply = connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(SERVICE),
            "Notify",
            &(APP_NAME, 0u32, "", APP_NAME, body, actions, hints, -1i32),
        )
        .await?;
    let id: u32 = reply.body().deserialize()?;
    posted.remember(id);
    Ok(id)
}

/// The text for a new version.
pub fn available_text(name: &str, version: &semver::Version) -> String {
    format!("{name} {version} is available.")
}

/// The text when an update left some of the person's own changes in place instead of replacing them.
pub const LEFT_ALONE_TEXT: &str =
    "The update is installed. A few things you had set up yourself were left as they are.";

/// The text after an automatic rollback.
pub const RESTORED_TEXT: &str = "The update could not start, so the previous version was restored.";

async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}

/// Whether a signal really came from the notification server: its sender is the connection that owns
/// the server's well-known name. A signal from any other connection is nobody's word, so a forged
/// `ActionInvoked` opens nothing. Either side unknown means no.
pub fn is_from_owner(sender: Option<&str>, owner: Option<&str>) -> bool {
    matches!((sender, owner), (Some(sender), Some(owner)) if sender == owner)
}

/// Who owns the notification server's name now, if anyone.
async fn server_owner(connection: &Connection) -> Option<String> {
    let bus = DBusProxy::new(connection).await.ok()?;
    let owner = bus
        .get_name_owner(BusName::try_from(SERVICE).ok()?)
        .await
        .ok()?;
    Some(owner.as_str().to_owned())
}

/// Whether `message` was sent by the notification server.
async fn from_server(connection: &Connection, message: &zbus::Message) -> bool {
    let sender = message.header().sender().map(|s| s.as_str().to_owned());
    is_from_owner(sender.as_deref(), server_owner(connection).await.as_deref())
}

/// Runs `settings_command` (split on spaces, no shell) whenever the person presses the button of
/// one of our notifications, as the notification server reported it. Ends when the bus connection ends.
pub async fn listen(
    connection: Connection,
    posted: Posted,
    settings_command: String,
) -> zbus::Result<()> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .interface(SERVICE)?
        .member("ActionInvoked")?
        .build();
    let mut stream = MessageStream::for_match_rule(rule, &connection, None).await?;
    while let Some(Ok(message)) = next(&mut stream).await {
        let Ok((id, key)) = message.body().deserialize::<(u32, String)>() else {
            continue;
        };
        if key == DEFAULT_ACTION && posted.knows(id) && from_server(&connection, &message).await {
            open_settings(&settings_command);
        }
    }
    Ok(())
}

fn open_settings(command: &str) {
    let mut words = command.split_whitespace();
    let Some(program) = words.next() else { return };
    if let Ok(mut child) = Command::new(program)
        .args(words)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}
