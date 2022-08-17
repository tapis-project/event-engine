use crate::errors::EngineError;
use zmq::Socket;

/// An event type in the application. Event types have a name, which be used to
/// specify subscriptions by components (plugins) in applications.
/// They also have a filter, which is a byte array that appears at the beginning
/// of every message of the type. The filter must uniquely determine messages to b
/// of said type.
pub trait EventType {
    /// return the name of the event type
    // TODO -- can we return String and Vec[u8]?
    fn get_name(&self) -> &'static str;

    /// compute the byte array filter for all messages of this event type.
    /// The filter must uniquely determine this event type.
    fn get_filter(&self) -> Result<&'static [u8], EngineError>;
}

/// Public API for defining events in an application.
/// Events are typed messages that can be sent over a ZMQ socket. Event are equipped
/// with functions to
pub trait Event {
    /// convert the event to a raw byte array
    fn to_bytes(&self) -> Result<Vec<u8>, EngineError>;

    /// send an event message to all subscribers
    fn send(&self, pub_socket: &mut Socket) -> Result<(), EngineError> {
        let data = self.to_bytes()?;
        pub_socket.send(data, 0)?;
        Ok(())
    }

    // TODO -- do we want to add from_bytes? probably do, even though not strictly required by the engine itself.
}
