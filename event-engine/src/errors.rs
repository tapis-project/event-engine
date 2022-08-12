use thiserror::Error;
use uuid::Uuid;

// pub enum ErrorCriticality {
//     Recoverable,
//     RestartRequired,
//     Crash,
// }

// pub struct Error {
//     user_message: String,
//     debug_message: String,
//     criticality: ErrorCriticality,
// }

// impl Error {
//     pub fn new(user_message: String, debug_message: String, criticality: ErrorCriticality) -> Self {
//         Error {
//             user_message,
//             debug_message,
//             criticality,
//         }
//     }
// }

// // conversions

// impl From<zmq::Error> for Error {
//     fn from(e: zmq::Error) -> Self {
//         Error {
//             user_message: "Socket level error".to_string(),
//             debug_message: format!("Socket level error from ZMQ; details: {}", e),
//             criticality: ErrorCriticality::Recoverable,
//         }
//     }
// }

/// EngineError enumerates all possible error types returned by the event-engine framework.
#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Engine could not create subscription socket")]
    SubSocketCreateError(#[from] zmq::Error),

    #[error("Engine could not bind subscription socket to TCP port {0}")]
    SubSocketTCPBindError(String),

    #[error("Engine could not bind subscription socket to inproc URL {0}")]
    SubSocketInProcBindError(String),

    #[error("Engine could not bind publish socket to TCP port {0}")]
    PubSocketTCPBindError(String),

    #[error("Engine could not bind publish socket to inproc URL {0}")]
    PubSocketInProcBindError(String),

    #[error("Engine failed to establish publish socket for plugin: {0}")]
    PluginPubSocketError(Uuid),

    #[error("Engine failed to establish subscription socket for plugin: {0}")]
    PluginSubSocketError(Uuid),

    #[error("Engine failed to set subscription bytes filter for event name {0} for plugin: {1}:")]
    PluginSubscriptionError(String, Uuid),

    #[error("Engine failed to establish sync socket for plugin: {0}, port: {1}")]
    PluginSyncSocketError(Uuid, i32),

    #[error("Engine could not set the subscription filter on the socket.")]
    EngineSetSubFilterError(),

    #[error("Plugin {0} failed to send the 'ready' sync message")]
    PluginSyncSendError(Uuid),

    #[error("Engine failed to create sync socket")]
    EngineSyncSocketCreateError(),

    #[error("Engine failed to bind to the sync socket on TCP port {0}")]
    EngineSyncSocketTCPBindError(i32),

    #[error("Engine failed to bind to the sync socket to inproc URL {0}")]
    EngineSyncSocketInprocBindError(String),

    #[error("Engine failed to receive a message on the sync socket.")]
    EngineSyncSocketMsgRcvError(),

    #[error("Engine failed to send a message on the sync socket.")]
    EngineSyncSocketSendRcvError(),
}
