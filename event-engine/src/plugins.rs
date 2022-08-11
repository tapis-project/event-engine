use uuid::Uuid;
use zmq::Socket;
use crate::{errors::EngineError, events::EventType};

/// Public API for defining the "plugins" or independent components of an application.
/// A plugin is an application component that defines its own event subscriptions and runs
/// as a standalone thread.
pub trait Plugin: Sync + 'static {
    /// the entry point for the plugin. the engine will start the plugin in its own 
    /// thread and execute this function. 
    /// the pub_socket is used by the plugin to publish new events.
    /// the sub_socket is used by the plugin to get events published by other plugins.
    fn start(&self, pub_socket: Socket, sub_socket: Socket) -> Result<(), EngineError>;

    /// Return the event subscriptions, as a vector of strings, that this plugin is interested
    /// in. 
    fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, EngineError>;

    /// Returns the unique id for this plugin
    fn get_id(&self) -> Uuid;

}