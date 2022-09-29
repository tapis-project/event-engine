//! Plugins are independent components of the application that create and consume events. Plugins can be either
//! internal, meaning they are written in Rust and run within the main application process as a child thread, or
//! external, meaning they run in a separate process from the main application. Internal plugins use an inprocess
//! communication mechanism to publish ans subscribe to events, while external plugins communicate with the main
//! application over TCP.
//!
//! The following traits provide the contracts for writing internal ans external plugins.
use crate::{errors::EngineError, events::EventType};
use log::{debug, info, error};
use uuid::Uuid;
use zmq::Socket;

/// Public API for defining the internal plugins. Internal plugns are plugins that run within the main application
/// process in a child thread. In particular, all internal plugins must be written in Rust.
pub trait Plugin: Sync + Send {
    /// The entry point for the plugin. The engine will start the plugin in its own
    /// thread and execute this function.
    /// the pub_socket is used by the plugin to publish new events.
    /// the sub_socket is used by the plugin to get events published by other plugins.
    fn start(&self, pub_socket: Socket, sub_socket: Socket) -> Result<(), EngineError>;

    /// Return the event subscriptions, as a vector of strings, that this plugin is interested
    /// in.
    fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, EngineError>;

    /// Returns the unique id for this plugin
    fn get_id(&self) -> Uuid;

    /// Returns the name for the plugin
    fn get_name(&self) -> String;
}

/// Public API for defining external plugins. External plugins are plugins that will run in a separate process from the main
/// application process.
/// In particular, all plugins written in languages other than Rust will be external plugins. These external plugins
/// still need to be registered on the main application.
pub trait ExternalPlugin: Send + Sync {
    /// A Rust start function for external plugins. This function provides an API for the external plugin process
    /// to retrieve events and publish new events. It also supports commands, such as "quit" which will allow it
    /// to shut down.
    /// This API is exposed on a zmq REP socket.
    fn start(
        &self,
        pub_socket: Socket,
        sub_socket: Socket,
        external_socket: Socket,
    ) -> Result<(), EngineError> {
        let plugin_id = self.get_id();
        debug!("{}: top of start function for external plugin", plugin_id);
        loop {
            debug!("{}: waiting for next message", plugin_id);
            // get the next message
            let msg = external_socket
                .recv_bytes(0)
                .map_err(|e| EngineError::EngineExtSocketRcvError(plugin_id, e))?;
            debug!("{}: got message from external plugin", plugin_id);

            if msg.is_ascii() {
                let msg_str_result = std::str::from_utf8(&msg);
                let msg = match msg_str_result {
                    Ok(m) => m,
                    Err(e) => {
                        // don't let a bad command crash everything, just log the error and continue
                        error!("{}: Got unexpected message from plugin: could not encode to utf8; error: {} message bytes: {:?}", plugin_id, e, msg);
                        // always need to reply
                        external_socket
                            .send("event-engine: bad msg", 0)
                            .map_err(|e| EngineError::EngineExtSocketSendError(plugin_id, e))?;
                        continue;
                    }
                };
                if msg == "plugin_command: quit" {
                    info!("{}: got quit message; exiting", plugin_id);
                    break;
                }
                if msg == "plugin_command: next_msg" {
                    debug!(
                        "{}: got next_msg command, retrieving next message",
                        plugin_id
                    );
                    // get the next message and reply with it over the external socket
                    let next_msg = sub_socket
                        .recv_bytes(0)
                        .map_err(|e| EngineError::EngineExtSubSocketRcvError(plugin_id, e))?;
                    debug!(
                        "{}: got next message, sengding over external socket",
                        plugin_id
                    );
                    external_socket
                        .send(next_msg, 0)
                        .map_err(|e| EngineError::EngineExtSocketSendError(plugin_id, e))?;
                    debug!("{}: external message sent, loop complete", plugin_id);
                    continue;
                }
            }
            // if we are here, we have not processed the message, so we assume it is a new event.
            // we publish it on the pub socket and then reply
            pub_socket
                .send(msg, 0)
                .map_err(|e| EngineError::EngineExtPubSocketSendError(plugin_id, e))?;
            external_socket
                .send("event-engine: msg published", 0)
                .map_err(|e| EngineError::EngineExtSocketSendError(plugin_id, e))?;
        }

        Ok(())
    }

    /// the tcp port on which the external plugin will retrieve and publish events.
    fn get_tcp_port(&self) -> i32;

    /// Return the event subscriptions, as a vector of strings, that this plugin is interested
    /// in.
    fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, EngineError>;

    /// Returns the unique id for this plugin
    fn get_id(&self) -> Uuid;

    /// Returns the name for the plugin
    fn get_name(&self) -> String;

}
