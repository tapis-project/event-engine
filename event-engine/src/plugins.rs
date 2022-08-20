use crate::{errors::EngineError, events::EventType};
use uuid::Uuid;
use zmq::Socket;

/// Public API for defining the "plugins" or independent components of an application.
/// A plugin is an application component that defines its own event subscriptions and runs
/// as a standalone thread. Both "internal" plugins and "external" plugins are supported.

/// API for internal plugins; i.e., plugins written in Rust and running in the main application 
/// process.
pub trait Plugin: Sync + Send {
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


/// API for external plugins. 
/// External plugins are plugins that will run in a separate process from the main application process.
/// In particular, all plugins written in languages other than Rust will be external plugins. These external plugins 
/// still need to be registered on the main application. 
pub trait ExternalPlugin: Send + Sync {

    /// A Rust start function for external plugins. This function provides an API for the external plugin process
    /// to retrieve events and publish new events. It also supports commands, such as "quit" which will allow it
    /// to shut down.
    /// This API is exposed on a zmq REP socket.
    fn start(&self, pub_socket: Socket, sub_socket: Socket, external_socket: Socket) -> Result<(), EngineError> {
        let plugin_id = self.get_id();
        println!("{}: top of start function for external plugin", plugin_id);
        loop {
            println!("{}: waiting for next message", plugin_id);
            // get the next message
            let msg = external_socket.recv_bytes(0)
                .map_err(|e| EngineError::EngineExtSocketRcvError(plugin_id, e))?;
            println!("{}: got message from external plugin", plugin_id);

            if msg.is_ascii() {
                let msg_str_result = std::str::from_utf8(&msg);
                let msg = match msg_str_result {
                    Ok(m) => m,
                    Err(e) => {
                        // don't let a bad command crash everything, just log the error and continue
                        println!("{}: Got unexpected message from plugin: could not encode to utf8; error: {} message bytes: {:?}", plugin_id, e, msg);
                        // always need to reply
                        external_socket.send("event-engine: bad msg", 0)
                            .map_err(|e| EngineError::EngineExtSocketSendError(plugin_id, e))?;
                        continue;
                    }
                };
                if msg == "plugin_command: quit".to_string() {
                    println!("{}: got quit message; exiting", plugin_id);
                    break;
                }
                if msg == "plugin_command: next_msg".to_string() {
                    println!("{}: got next_msg command, retrieving next message", plugin_id);
                    // get the next message and reply with it over the external socket
                    let next_msg = sub_socket.recv_bytes(0)
                        .map_err(|e| EngineError::EngineExtSubSocketRcvError(plugin_id, e))?;
                    println!("{}: got next message, sengding over external socket", plugin_id);
                    external_socket.send(next_msg, 0)
                        .map_err(|e| EngineError::EngineExtSocketSendError(plugin_id, e))?;
                    println!("{}: external message sent, loop complete", plugin_id);
                    continue;
                }
            }
            // if we are here, we have not processed the message, so we assume it is a new event.
            // we publish it on the pub socket and then reply
            pub_socket.send(msg, 0)
                .map_err(|e| EngineError::EngineExtPubSocketSendError(plugin_id, e))?;
            external_socket.send("event-engine: msg published", 0)
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

}
