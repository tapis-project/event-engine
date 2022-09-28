/// # Event Engine
/// This library provides a framework for writing event-based applications that utilize a plugin architecture.
/// Applications built with `event-engine` are written in Rust but can utilize plugins in written in multiple languages.
///
/// Events correspond to statically typed messages that can be transmitted over a socket and are the basic
/// mechanism of communication between plugins of the application. Each application defines its own event types
/// which include mechanisms for serializing and deserializing a message to/from bytes. No assumption is made
/// about the serialization format used for events, and different event types can utilize different formats.
///
/// Plugins are independent components of an application that create and consume events. Each plugin defines
/// the event types it is interested in, and the core `event-engine` proxies events across the plugins of an
/// application using a publish-subscribe pattern. Each application defines its own plugins which can either be
/// internal or external. Internal plugins run as child threads within the main (Rust) application process and
/// are necessarily written in Rust. External plugins run as separate OS processes and can be written in any
/// language.
///
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
};

use errors::EngineError;
use log::{error, info};
use plugins::{ExternalPlugin, Plugin};
use uuid::Uuid;
use zmq::{Context, Socket};

pub mod errors;
pub mod events;
pub mod plugins;

/// Configuration for the `event-engine` application.
pub struct AppConfig {
    pub publish_port: i32,
    pub subscribe_port: i32,
}

// configuration data related to creating the sockets used by the engine.
struct SocketData {
    pub_socket_inproc_url: String,
    sub_socket_inproc_url: String,
    sync_socket_port: i32,
    sync_inproc_url: String,
}

impl Default for SocketData {
    fn default() -> Self {
        Self {
            pub_socket_inproc_url: "inproc://messages".to_string(),
            sub_socket_inproc_url: "inproc://events".to_string(),
            sync_socket_port: 5000,
            sync_inproc_url: "inproc://sync".to_string(),
        }
    }
}

/// The `event-engine` application object.
pub struct App {
    pub plugins: Vec<Arc<Box<dyn Plugin>>>,
    pub external_plugins: Vec<Arc<Box<dyn ExternalPlugin>>>,
    pub app_config: AppConfig,
    pub context: Context,
}

impl Default for App {
    fn default() -> Self {
        App {
            plugins: vec![],
            external_plugins: vec![],
            app_config: AppConfig {
                publish_port: 5559,
                subscribe_port: 5560,
            },
            context: Context::new(),
        }
    }
}

impl App {
    pub fn new(publish_port: i32, subscribe_port: i32) -> Self {
        let app_config = AppConfig {
            publish_port,
            subscribe_port,
        };
        App {
            app_config,
            ..Default::default()
        }
    }

    /// create the zmq socket to be used for 'outgoing' events; i.e., the events the plugins will receive
    /// on the subscriptions socket
    fn get_outgoing_socket(&self) -> Result<Socket, EngineError> {
        let socket_data = SocketData::default();
        let sub_socket_tcp_url = format!("tcp://*:{}", self.app_config.subscribe_port);
        // from the engine's perspective, this socket is a PUB
        let outgoing = self.context.socket(zmq::PUB)?;
        // bind the socket to the TCP port
        outgoing.bind(&sub_socket_tcp_url).map_err(|e| {
            EngineError::SubSocketTCPBindError(self.app_config.subscribe_port.to_string(), e)
        })?;
        // bind the socket to the inproc URL
        outgoing
            .bind(&socket_data.sub_socket_inproc_url)
            .map_err(|_e| {
                EngineError::SubSocketInProcBindError(socket_data.sub_socket_inproc_url.to_string())
            })?;
        Ok(outgoing)
    }

    /// create the zmq socket to be used for 'incoming' events; i.e., events published by the plugins
    fn get_incoming_socket(&self) -> Result<Socket, EngineError> {
        let socket_data = SocketData::default();
        let pub_socket_tcp_url = format!("tcp://*:{}", self.app_config.publish_port);
        let incoming = self.context.socket(zmq::SUB)?;
        // bind the socket to the TCP port
        incoming.bind(&pub_socket_tcp_url).map_err(|_e| {
            EngineError::PubSocketTCPBindError(self.app_config.subscribe_port.to_string())
        })?;
        // bind the socket to the inproc URL
        incoming
            .bind(&socket_data.pub_socket_inproc_url)
            .map_err(|_e| {
                EngineError::PubSocketInProcBindError(socket_data.pub_socket_inproc_url.to_string())
            })?;
        // subscribe to all events
        let filter = String::new();
        incoming
            .set_subscribe(filter.as_bytes())
            .map_err(EngineError::EngineSetSubFilterAllError)?;
        Ok(incoming)
    }

    /// start a plugin. this function does the following, from within a new thread:
    ///   1) creates the publish and subscribe socket objects that the plugin will use for events.
    ///   2) configures the plugin's sub socket with the plugin's subscriptions
    ///   3) creates the sync socket that the plugin will use to sync with the engine
    ///   4) clones the socket objects, starts a thread, and moves ownership of the cloned objects into the thread.
    ///   5) sends the 'ready' message on the sync socket.
    ///   6) waits for the engine's reply on the sync socket.
    ///   7) executes the plugin's start() function.
    fn start_plugin(
        &self,
        context: &zmq::Context,
        plugin: Arc<Box<dyn Plugin>>,
    ) -> Result<JoinHandle<()>, EngineError> {
        let socket_data = SocketData::default();

        // we need to clone the zmq context so that the thread can take ownership and avoid static lifetime
        // requirements on the original context object. note also that creating a brand new context within the
        // thread does not work -- the inproc endpoints will not be shared.
        let context_clone = context.clone();
        let thread_handle = thread::spawn(move || {
            println!("plugin {} thread started.", plugin.get_id());

            // TODO -- in the blocked code that follows, we use a series of .unwrap()s to effectively crash
            // the entire thread when we encounter an EngineError. Using the ? operator to "bubble" up the errors
            // naturally does not work. We should explore alternatives though

            // -------------------------------------------------------------------------------------------------

            // Create the socket that plugin will use to publish new events
            let pub_socket = context_clone
                .socket(zmq::PUB)
                .map_err(|_e| EngineError::PluginPubSocketError(plugin.get_id()))
                .unwrap();
            pub_socket
                .connect(&socket_data.pub_socket_inproc_url)
                .map_err(|_e| EngineError::PluginPubSocketError(plugin.get_id()))
                .unwrap();
            println!("plugin {} connected to pub socket.", plugin.get_id());

            // Create the socket that plugin will use to subscribe to events
            let sub_socket = context_clone
                .socket(zmq::SUB)
                .map_err(|_e| EngineError::PluginSubSocketError(plugin.get_id()))
                .unwrap();
            sub_socket
                .connect(&socket_data.sub_socket_inproc_url)
                .map_err(|_e| EngineError::PluginPubSocketError(plugin.get_id()))
                .unwrap();

            // Subscribe only to events of interest for this plugin
            for sub in plugin.get_subscriptions().unwrap() {
                let filter = sub
                    .get_filter()
                    .map_err(|_e| EngineError::EngineSetSubFilterError())
                    .unwrap();
                println!(
                    "Engine setting subscription filter {:?} for plugin: {}",
                    filter,
                    plugin.get_id()
                );
                // TODO -- the following error handling doesn't work; compiler complains, "cannot return value referencing
                // local data"; that is, we cannot return the EngineError..
                // let filter = sub.get_filter()?;
                sub_socket
                    .set_subscribe(&filter)
                    .map_err(|_e| {
                        EngineError::PluginSubscriptionError(sub.get_name(), plugin.get_id())
                    })
                    .unwrap();
            }
            println!(
                "plugin {} connected to sub socket with subscriptions set.",
                plugin.get_id()
            );

            // Create the sync socket that plugin will use to sync with engine and other plugins
            let sync = context_clone
                .socket(zmq::REQ)
                .map_err(|_e| {
                    EngineError::PluginSyncSocketError(
                        plugin.get_id(),
                        socket_data.sync_socket_port,
                    )
                })
                .unwrap();
            // connect the sync socket to the inproc URL.
            // the URL must be different for EACH plugin. this is because the engine receives ALL sync ready messages
            // from all plugins before sending any replies, and the zmq REQ-REP sockets only allow one request per
            // reply.
            // NOTE: since this sync object is for use in the plugin (running in a thread), it will always
            // be used via inproc (threads use inproc).. therefore, we do not connect to the TCP URL.
            let plugin_sync_socket_inproc_url =
                format!("{}-{}", &socket_data.sync_inproc_url, plugin.get_id());

            sync.connect(&plugin_sync_socket_inproc_url)
                .map_err(|_e| {
                    EngineError::PluginSyncSocketError(
                        plugin.get_id(),
                        socket_data.sync_socket_port,
                    )
                })
                .unwrap();
            println!(
                "plugin {} connected to sync socket at URL: {}.",
                plugin.get_id(),
                plugin_sync_socket_inproc_url
            );

            // connect to and send sync message on sync socket
            let msg = "ready";
            sync.send(msg, 0)
                .expect("Could not send ready message on thread for plugin; crashing!");
            println!("plugin {} sent ready message.", plugin.get_id());

            // TODO -- couldn't get this error handling to work...
            // .map_err(|_e| EngineError::PluginSyncSendError(plugin.get_id()))?;

            // blocking call to wait for reply from engine
            let _msg = sync
                .recv_msg(0)
                .expect("plugin got error trying to receive sync reply; crashing!");

            println!(
                "plugin {} received reply from ready message. Executing start function...",
                plugin.get_id()
            );

            // now execute the actual plugin function
            plugin.start(pub_socket, sub_socket).unwrap();
        });
        Ok(thread_handle)
    }

    fn start_external_plugin(
        &self,
        context: &zmq::Context,
        plugin: Arc<Box<dyn ExternalPlugin>>,
    ) -> Result<JoinHandle<()>, EngineError> {
        let socket_data = SocketData::default();

        // we need to clone the zmq context so that the thread can take ownership and avoid static lifetime
        // requirements on the original context object. note also that creating a brand new context within the
        // thread does not work -- the inproc endpoints will not be shared.
        let context_clone = context.clone();
        let thread_handle = thread::spawn(move || {
            println!("external plugin {} thread started.", plugin.get_id());

            // TODO -- in the blocked code that follows, we use a series of .unwrap()s to effectively crash
            // the entire thread when we encounter an EngineError. Using the ? operator to "bubble" up the errors
            // naturally does not work. We should explore alternatives though

            // -------------------------------------------------------------------------------------------------

            // Create the external socket used to communicate with the external plugin process
            let external_socket = context_clone
                .socket(zmq::REP)
                .map_err(|e| EngineError::PluginExternalSocketError(plugin.get_id(), e))
                .unwrap();
            let external_tcp_url = format!("tcp://*:{}", plugin.get_tcp_port());

            external_socket
                .bind(&external_tcp_url)
                .map_err(|e| EngineError::PluginExternalSocketError(plugin.get_id(), e))
                .unwrap();
            println!("plugin bound to external TCP socket: {}", external_tcp_url);

            // Create the socket that plugin will use to publish new events
            let pub_socket = context_clone
                .socket(zmq::PUB)
                .map_err(|_e| EngineError::PluginPubSocketError(plugin.get_id()))
                .unwrap();
            pub_socket
                .connect(&socket_data.pub_socket_inproc_url)
                .map_err(|_e| EngineError::PluginPubSocketError(plugin.get_id()))
                .unwrap();
            println!("plugin {} connected to pub socket.", plugin.get_id());

            // Create the socket that plugin will use to subscribe to events
            let sub_socket = context_clone
                .socket(zmq::SUB)
                .map_err(|_e| EngineError::PluginSubSocketError(plugin.get_id()))
                .unwrap();
            sub_socket
                .connect(&socket_data.sub_socket_inproc_url)
                .map_err(|_e| EngineError::PluginPubSocketError(plugin.get_id()))
                .unwrap();

            // Subscribe only to events of interest for this plugin
            for sub in plugin.get_subscriptions().unwrap() {
                let filter = sub
                    .get_filter()
                    .map_err(|_e| EngineError::EngineSetSubFilterError())
                    .unwrap();
                println!("Engine setting subscription filter {:?}", filter);
                // TODO -- the following error handling doesn't work; compiler complains, "cannot return value referencing
                // local data"; that is, we cannot return the EngineError..
                // let filter = sub.get_filter()?;
                sub_socket
                    .set_subscribe(&filter)
                    .map_err(|_e| {
                        EngineError::PluginSubscriptionError(sub.get_name(), plugin.get_id())
                    })
                    .unwrap();
            }
            println!(
                "external plugin {} connected to sub socket with subscriptions set.",
                plugin.get_id()
            );

            // Create the sync socket that plugin will use to sync with engine and other plugins
            let sync = context_clone
                .socket(zmq::REQ)
                .map_err(|_e| {
                    EngineError::PluginSyncSocketError(
                        plugin.get_id(),
                        socket_data.sync_socket_port,
                    )
                })
                .unwrap();
            // connect the sync socket to the inproc URL.
            // the URL must be different for EACH plugin. this is because the engine receives ALL sync ready messages
            // from all plugins before sending any replies, and the zmq REQ-REP sockets only allow one request per
            // reply.
            // NOTE: since this sync object is for use in the plugin (running in a thread), it will always
            // be used via inproc (threads use inproc).. therefore, we do not connect to the TCP URL.
            let plugin_sync_socket_inproc_url =
                format!("{}-{}", &socket_data.sync_inproc_url, plugin.get_id());

            sync.connect(&plugin_sync_socket_inproc_url)
                .map_err(|_e| {
                    EngineError::PluginSyncSocketError(
                        plugin.get_id(),
                        socket_data.sync_socket_port,
                    )
                })
                .unwrap();
            println!(
                "plugin {} connected to sync socket at URL: {}.",
                plugin.get_id(),
                plugin_sync_socket_inproc_url
            );

            // connect to and send sync message on sync socket
            let msg = "ready";
            sync.send(msg, 0)
                .expect("Could not send ready message on thread for plugin; crashing!");
            println!("plugin {} sent ready message.", plugin.get_id());

            // TODO -- couldn't get this error handling to work...
            // .map_err(|_e| EngineError::PluginSyncSendError(plugin.get_id()))?;

            // blocking call to wait for reply from engine
            let _msg = sync
                .recv_msg(0)
                .expect("plugin got error trying to receive sync reply; crashing!");

            println!(
                "plugin {} received reply from ready message. Executing start function...",
                plugin.get_id()
            );

            // now execute the actual plugin function
            plugin
                .start(pub_socket, sub_socket, external_socket)
                .unwrap();
        });

        Ok(thread_handle)
    }

    /// this function synchronizes all plugins to handle plugins that might start up more slowly than others.
    /// it utilizes a set of REQ-REP zmq sockets -- one for each plugin.
    /// the basic algorithm is:
    ///   1) wait to receive ready messages from all plugins; it does this by doing a recv on each socket
    ///   2) send an OK to all plugins
    /// note: that we must use different sync sockets since the zmq REQ-REP socket only allows for the receipt of
    /// one message before sending a reply and we must recieve 'ready' messages from all plugins before replying.
    /// note: this function currently DOES NOT sync external plugins. this is left as a TODO.
    fn sync_plugins(&self, context: &zmq::Context) -> Result<(), EngineError> {
        let socket_data = SocketData::default();
        // set of all sync sockets engine will use
        let mut sync_sockets = Vec::<zmq::Socket>::new();

        // iterate through each plugin, creating a sync socket for it using its id, and waiting
        // for a ready message. all we need for this is the plugin_id, so we first build a vector of all
        // plugin_id's, internal and external.
        let mut plugin_ids: Vec<Uuid> = self.plugins.iter().map(|x| x.get_id()).collect();
        let mut external_plugin_ids: Vec<Uuid> =
            self.external_plugins.iter().map(|x| x.get_id()).collect();
        plugin_ids.append(&mut external_plugin_ids);
        println!("Engine will now sync these plugins: {:?}", plugin_ids);

        for plugin_id in &plugin_ids {
            let sync_socket = context
                .socket(zmq::REP)
                .map_err(|_e| EngineError::EngineSyncSocketCreateError())?;

            // bind sync socket to inproc URL
            let plugin_sync_socket_inproc_url =
                format!("{}-{}", &socket_data.sync_inproc_url, plugin_id);
            println!(
                "Engine binding to sync inproc URL: {}",
                &plugin_sync_socket_inproc_url
            );
            sync_socket
                .bind(&plugin_sync_socket_inproc_url)
                .map_err(|_e| {
                    EngineError::EngineSyncSocketInprocBindError(plugin_sync_socket_inproc_url)
                })?;
            // receive ready message from plugin
            let _msg = sync_socket
                .recv_msg(0)
                .map_err(EngineError::EngineSyncSocketMsgRcvError)?;
            println!("Engine received a ready message from plugin {}", plugin_id);
            sync_sockets.push(sync_socket);
        }

        println!("Engine received all ready messages; now sending replies.");

        // send a reply to all plugins
        let mut msg_sent = 0;
        while msg_sent < plugin_ids.len() {
            let reply = "ok";
            let sync_socket = sync_sockets
                .pop()
                .ok_or(EngineError::EngineSyncSocketPopError())?;
            sync_socket
                .send(reply, 0)
                .map_err(EngineError::EngineSyncSocketSendRcvError)?;
            msg_sent += 1;
            println!("Engine sent a reply");
        }
        println!("All plugins have been synced");

        Ok(())
    }

    fn start_plugins(&self) -> Result<Vec<JoinHandle<()>>, EngineError> {
        // call start_plugin with the zmq context and the config for each plugin,
        // as defined in the PLUGINS constant
        let mut thread_handles = vec![];
        for plugin in &self.plugins {
            let p = Arc::clone(plugin);
            thread_handles.push(self.start_plugin(&self.context, p)?);
        }

        Ok(thread_handles)
    }

    fn start_external_plugins(&self) -> Result<Vec<JoinHandle<()>>, EngineError> {
        // call start_plugin with the zmq context and the config for each plugin,
        // as defined in the PLUGINS constant
        let mut thread_handles = vec![];
        for plugin in &self.external_plugins {
            let p = Arc::clone(plugin);
            thread_handles.push(self.start_external_plugin(&self.context, p)?);
        }
        Ok(thread_handles)
    }

    /// Add a new plugin to the app
    pub fn register_plugin(mut self, plugin: Arc<Box<dyn Plugin>>) -> Self {
        self.plugins.push(plugin);
        self
    }

    /// Add a new external plugin to the app
    pub fn register_external_plugin(mut self, plugin: Arc<Box<dyn ExternalPlugin>>) -> Self {
        self.external_plugins.push(plugin);
        self
    }

    pub fn run(self) -> Result<(), EngineError> {
        println!("Engine starting application with {} plugins and {} external plugins on publish port: {} and subscribe port: {}.", self.plugins.len(), self.external_plugins.len(), self.app_config.publish_port, self.app_config.subscribe_port);
        // incoming and outgoing sockets for the engine
        let outgoing = self.get_outgoing_socket()?;
        let incoming = self.get_incoming_socket()?;

        println!("Engine starting zmq proxy.");

        let _proxy_thread = thread::spawn(move || {
            let _result = zmq::proxy(&incoming, &outgoing)
                .expect("Engine got error running proxy; socket was closed?");
        });

        println!("Engine has started proxy thread. Will now start and sync plugins");

        // start plugins in their own thread
        let plugin_thread_handles = self.start_plugins()?;

        // start external plugins
        let external_plugin_thread_handles = self.start_external_plugins()?;

        // sync all plugins (internal and external)
        self.sync_plugins(&self.context)?;

        println!("All plugins started and synced; Will now wait for plugins to exit...");

        // join all of the plugin threads, and when they are all complete, we can kill the entire
        // program
        for h in plugin_thread_handles {
            h.join().unwrap();
        }

        println!(
            "Engine joined all internal plugin threads; will now join external plugin threads."
        );
        for h in external_plugin_thread_handles {
            h.join().unwrap();
        }
        // join all external plugins as well; if we don't there are still race conditions because external
        // plugins could be slow to start and the main program could quit before they have received messages

        println!("Engine joined all plugin threads.. ready to shut down.");
        // all plugins have exited so let's kill the proxy thread now
        // TODO -- is there a way to shut down the proxy thread using the handle? if we just exit
        // here without terminating it, is that ok?

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use std::{str, sync::Arc, vec};

    use zmq::Socket;

    use crate::{
        events::{Event, EventType},
        plugins::Plugin,
        App,
    };

    // Here we provide two simple, example event types. TypeA, which has a single string field,
    // and TypeB which has a single integer field.
    struct TypeAEventType {}
    impl EventType for TypeAEventType {
        fn get_name(&self) -> String {
            let s = "TypeA";
            s.to_string()
        }

        fn get_filter(&self) -> Result<Vec<u8>, crate::errors::EngineError> {
            // just return the bytes associated with the name.
            Ok(self.get_name().as_bytes().to_vec())
        }
    }

    // Example event for event type TypeA
    struct TypeAEvent {
        message: String,
    }

    impl Event for TypeAEvent {
        fn to_bytes(&self) -> Result<Vec<u8>, crate::errors::EngineError> {
            let type_a = TypeAEventType {};
            // The byte array begins with the filter and then adds the message
            let result = [
                type_a.get_filter().unwrap(),
                self.message.as_bytes().to_vec(),
            ]
            .concat();
            Ok(result)
        }

        fn from_bytes(mut b: Vec<u8>) -> Result<TypeAEvent, Box<(dyn std::error::Error + 'static)>> {
            // remove the first 5 bytes which are the message type
            for _i in 1..5 {
                b.remove(0);
            }
            let msg = str::from_utf8(&b).unwrap();
            Ok(TypeAEvent {
                message: msg.to_string(),
            })
        }
    }

    // The second event type, TypeB.
    struct TypeBEventType {}
    impl EventType for TypeBEventType {
        fn get_name(&self) -> String {
            let s = "TypeB";
            s.to_string()
        }

        fn get_filter(&self) -> Result<Vec<u8>, crate::errors::EngineError> {
            // just return the bytes associated with the name.
            Ok(self.get_name().as_bytes().to_vec())
        }
    }

    // Event for event type TypeB
    struct TypeBEvent {
        count: usize,
    }

    impl Event for TypeBEvent {
        fn to_bytes(&self) -> Result<Vec<u8>, crate::errors::EngineError> {
            // this is a TypeB event
            let type_b = TypeBEventType {};
            let message = format!("{}", self.count);
            // The byte array begins with the filter and then adds the message
            let result = [type_b.get_filter().unwrap(), message.as_bytes().to_vec()].concat();
            Ok(result)
        }

        fn from_bytes(mut b: Vec<u8>) -> Result<TypeBEvent, Box<(dyn std::error::Error + 'static)>> {
            // remove the first 5 bytes which are the message type
            for _i in 0..5 {
                b.remove(0);
            }
            let msg = str::from_utf8(&b).unwrap();
            Ok(TypeBEvent {
                count: msg.to_string().parse().unwrap(),
            })
        }
    }

    // Plugin examples.
    // Example of a "message producer" plugin. This plugin produces 5 strings and sends them as TypeA events.
    // It sends the 5 events as fast as possible.
    // It also subscribes to TypeB events, which are sent by the "counter" pluging in response to TypeA evnts.
    // After sending its 5 events, it then receives all of the TypeB events (there should b exactly 5).
    struct MsgProducerPlugin {
        id: uuid::Uuid,
    }
    impl MsgProducerPlugin {
        fn new() -> Self {
            MsgProducerPlugin {
                id: uuid::Uuid::new_v4(),
            }
        }
    }
    impl Plugin for MsgProducerPlugin {
        fn start(
            &self,
            pub_socket: Socket,
            sub_socket: Socket,
        ) -> Result<(), crate::errors::EngineError> {
            println!(
                "MsgProducer (plugin id {}) start function starting...",
                self.get_id()
            );
            println!(
                "MsgProducer (plugin id {}) finished 1 second sleep",
                self.get_id()
            );

            // send 5 messages
            let mut total_messages_sent = 0;
            while total_messages_sent < 5 {
                let message = format!("This is message {}", total_messages_sent);
                let m = TypeAEvent { message };
                let data = m.to_bytes().unwrap();
                println!("MsgProducer sending bytes: {:?}", data);
                pub_socket.send(data, 0).unwrap();
                total_messages_sent += 1;
                println!(
                    "MsgProducer sent TypeA event message: {}",
                    total_messages_sent
                );
            }
            println!("MsgProducer has sent all TypeA event messages, now waiting to receive TypeB events");

            // now get the TypeB events
            let mut total_messages_read = 0;
            while total_messages_read < 5 {
                // get the bytes of a new message; it should be of TypeB
                let b = sub_socket.recv_bytes(0).unwrap();
                println!("MsgProducer received TypeB message; bytes: {:?}", b);
                let event_msg = TypeBEvent::from_bytes(b).unwrap();
                let count = event_msg.count;
                println!("Got a type B message; count was: {}", count);
                total_messages_read += 1;
                println!(
                    "MsgProducer received TypeB event message: {}",
                    total_messages_read
                );
            }
            println!("MsgProducer has received all TypeB event messages; now exiting.");

            Ok(())
        }

        fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, crate::errors::EngineError> {
            Ok(vec![Box::new(TypeBEventType {})])
        }

        fn get_id(&self) -> uuid::Uuid {
            self.id
        }
    }

    // Example of a "counter" plugin. This plugin subscribes to TypeA events and computes the character count in the message. It then
    // produces a typeB event with the character count it computed.

    struct CounterPlugin {
        id: uuid::Uuid,
    }
    impl CounterPlugin {
        fn new() -> Self {
            CounterPlugin {
                id: uuid::Uuid::new_v4(),
            }
        }
    }
    impl Plugin for CounterPlugin {
        fn start(
            &self,
            pub_socket: Socket,
            sub_socket: Socket,
        ) -> Result<(), crate::errors::EngineError> {
            println!(
                "Counter (plugin id {}) start function starting...",
                self.get_id()
            );
            // compute the counts of the first 5 messages
            let mut total_messages_read = 0;
            while total_messages_read < 5 {
                // get the bytes of a new message; it should be of TypeA
                let b = sub_socket.recv_bytes(0).unwrap();
                let event_msg = TypeAEvent::from_bytes(b).unwrap();
                let count = event_msg.message.len();
                total_messages_read += 1;
                println!(
                    "Counter plugin received TypeA message: {}",
                    total_messages_read
                );
                // send a TypeB event
                let m = TypeBEvent { count };
                let data = m.to_bytes().unwrap();
                pub_socket.send(data, 0).unwrap();
                println!("Counter plugin sent TypeB message: {}", total_messages_read);
            }
            println!("Counter plugin has sent all TypeB messages; now exiting.");

            Ok(())
        }

        fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, crate::errors::EngineError> {
            Ok(vec![Box::new(TypeAEventType {})])
        }

        fn get_id(&self) -> uuid::Uuid {
            self.id
        }
    }

    // basic test that we can register plugins and execute app.run() and that the program terminates.
    #[test]
    fn test_run_app() -> Result<(), String> {
        // the plugins for our app
        println!("inside the test_run_app");
        let msg_producer = MsgProducerPlugin::new();
        let counter = CounterPlugin::new();
        println!("plugins for test_run_app configured");
        let app: App = App::new(5559, 5560);
        app.register_plugin(Arc::new(Box::new(msg_producer)))
            .register_plugin(Arc::new(Box::new(counter)))
            // .register_external_plugin(publish_port, subscribe_port)
            .run()
            .map_err(|e| format!("Got error from Engine! Details: {}", e))?;
        println!("returned from test_run_app.run()");
        Ok(())
    }

    // ----- event filters -----
    // test that plugin gets the messages that it subscribed to

    // test that plugin does NOT get messages it did not subscribe to

    // ----- event to_bytes() and from_bytes() -----
    // test that a message received is byte-for-byte identical to the message that was sent
}
