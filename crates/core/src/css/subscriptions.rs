//! Change notification system.
//!
//! Broadcasts CSS property changes and DOM updates to subscribers.
//! Subscribers decide what to do with the notifications based on their own
//! visibility tracking and property interest logic.

use crate::NodeId;
use lightningcss::properties::Property;
use std::sync::RwLock;

/// DOM broadcast events sent to subscribers.
#[derive(Debug, Clone, Copy)]
pub enum DomBroadcast {
    /// A new node was created and attached to a parent.
    CreateNode { node: NodeId, parent: NodeId },
}

/// Trait for subscribers that receive property and DOM notifications.
pub trait Subscriber: Send + Sync {
    /// Called when a CSS property changes on a node.
    fn on_property(&self, node: NodeId, property: &Property<'static>);

    /// Called when the DOM structure changes.
    fn on_dom(&self, update: DomBroadcast);
}

/// Change notification broadcaster.
/// Notifies all registered subscribers when CSS properties or DOM structure change.
pub struct Subscriptions {
    subscribers: RwLock<Vec<Box<dyn Subscriber>>>,
}

impl Default for Subscriptions {
    fn default() -> Self {
        Self::new()
    }
}

impl Subscriptions {
    /// Create a new empty subscription broadcaster.
    pub fn new() -> Self {
        Self {
            subscribers: RwLock::new(Vec::new()),
        }
    }

    /// Add a subscriber to receive notifications.
    pub fn add_subscriber(&self, subscriber: Box<dyn Subscriber>) {
        if let Ok(mut subs) = self.subscribers.write() {
            subs.push(subscriber);
        }
    }

    /// Notify all subscribers that a property changed on a node.
    pub fn notify_property(&self, node: NodeId, property: &Property<'static>) {
        if let Ok(subs) = self.subscribers.read() {
            for subscriber in subs.iter() {
                subscriber.on_property(node, property);
            }
        }
    }

    /// Notify all subscribers of a DOM update.
    pub fn notify_dom(&self, update: DomBroadcast) {
        if let Ok(subs) = self.subscribers.read() {
            for subscriber in subs.iter() {
                subscriber.on_dom(update);
            }
        }
    }
}
