use std::{
    sync::{Arc, Mutex},
    thread,
};

use errors::EngineError;
use plugins::Plugin;
use zmq::{Context, Socket};

pub mod errors;
pub mod events;
pub mod plugins;

// configuration for the app
pub struct AppConfig {
    pub publish_port: i32,
    pub subscribe_port: i32,
}

// configuration data related to creating the sockets used by the enting
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

pub struct App {
    pub plugins: Vec<Arc<Mutex<Box<dyn Plugin>>>>,
    pub app_config: AppConfig,
    pub context: Context,
}

impl Default for App {
    fn default() -> Self {
        App {
            plugins: vec![],
            //plugins: Arc::new(Mutex::new(vec![])),
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

    // create the zmq socket to be used for 'outgoing' events; i.e., the events the plugins will receive
    // on the subscriptions socket
    fn get_sub_socket(&self) -> Result<Socket, EngineError> {
        let socket_data = SocketData::default();
        let sub_socket_tcp_url = format!("tcp://*:{}", self.app_config.subscribe_port);
        let outgoing = self.context.socket(zmq::PUB)?;
        // bind the socket to the TCP port
        outgoing.bind(&sub_socket_tcp_url).map_err(|_e| {
            EngineError::SubSocketTCPBindError(self.app_config.subscribe_port.to_string())
        })?;
        // bind the socket to the inproc URL
        outgoing
            .bind(&socket_data.sub_socket_inproc_url)
            .map_err(|_e| {
                EngineError::SubSocketInProcBindError(socket_data.sub_socket_inproc_url.to_string())
            })?;
        Ok(outgoing)
    }

    // create the zmq socket to be used for 'incoming' events; i.e., events published by the plugins
    fn get_pub_socket(&self) -> Result<Socket, EngineError> {
        let socket_data = SocketData::default();
        let pub_socket_tcp_url = format!("tcp://*:{}", self.app_config.publish_port);
        let outgoing = self.context.socket(zmq::PUB)?;
        // bind the socket to the TCP port
        outgoing.bind(&pub_socket_tcp_url).map_err(|_e| {
            EngineError::PubSocketTCPBindError(self.app_config.subscribe_port.to_string())
        })?;
        // bind the socket to the inproc URL
        outgoing
            .bind(&socket_data.pub_socket_inproc_url)
            .map_err(|_e| {
                EngineError::PubSocketInProcBindError(socket_data.pub_socket_inproc_url.to_string())
            })?;
        Ok(outgoing)
    }

    fn start_plugin(
        &self,
        context: &zmq::Context,
        plugin: &'static Arc<Mutex<Box<dyn Plugin>>>,
    ) -> Result<(), EngineError> {
        let socket_data = SocketData::default();
        // Create the socket that plugin will use to publish new events
        let pub_socket = context
            .socket(zmq::PUB)
            .map_err(|_e| EngineError::PluginPubSocketError(plugin.lock().unwrap().get_id()))?;
        pub_socket
            .connect(&socket_data.pub_socket_inproc_url)
            .map_err(|_e| EngineError::PluginPubSocketError(plugin.lock().unwrap().get_id()))?;
        println!(
            "plugin {} connected to pub socket.",
            plugin.lock().unwrap().get_id()
        );

        // Create the socket that plugin will use to subscribe to events
        let sub_socket = context
            .socket(zmq::SUB)
            .map_err(|_e| EngineError::PluginSubSocketError(plugin.lock().unwrap().get_id()))?;
        // Subscribe only to events of interest for this plugin
        for sub in plugin.lock().unwrap().get_subscriptions()? {
            let filter = sub
                .get_filter()
                .map_err(|_e| EngineError::EngineSetSubFilterError())?;

            // TODO -- the following error handling doesn't work; compiler complains, "cannot return value referencing
            // local data"; that is, we cannot return the EngineError..
            // let filter = sub.get_filter()?;
            sub_socket.set_subscribe(filter).map_err(|_e| {
                EngineError::PluginSubscriptionError(
                    sub.get_name().to_string(),
                    plugin.lock().unwrap().get_id(),
                )
            })?;
        }
        // Create the sync socket that plugin will use to sync with engine and other plugins
        let sync = context.socket(zmq::REQ).map_err(|_e| {
            EngineError::PluginSyncSocketError(
                plugin.lock().unwrap().get_id(),
                socket_data.sync_socket_port,
            )
        })?;
        // connect the sync socket to the inproc URL
        // NOTE: since this sync object is for use in the plugin (running in a thread), it will always
        // be used via inproc (threads use inproc).. therefore, we do not connect to the TCP URL.
        sync.connect(&socket_data.sync_inproc_url).map_err(|_e| {
            EngineError::PluginSyncSocketError(
                plugin.lock().unwrap().get_id(),
                socket_data.sync_socket_port,
            )
        })?;

        // start the plugin thread. we start all plugin threads before the call to sync_plugins
        // so that plugins will be starting up and able to send the 'ok' message
        thread::spawn(move || {
            // connect to and send sync message on sync socket
            let msg = "ready";
            sync.send(msg, 0)
                .expect("Could not start thread for plugin; crashing!");
            // TODO -- couldn't get this error handling to work...
            // .map_err(|_e| EngineError::PluginSyncSendError(plugin.get_id()))?;

            // blocking call to wait for reply from engine
            let _msg = sync
                .recv_msg(0)
                .expect("plugin got error trying to receive sync reply; crashing!");

            // now execute the actual plugin function
            plugin
                .lock()
                .unwrap()
                .start(pub_socket, sub_socket)
                .unwrap();
        });
        Ok(())
    }

    fn sync_plugins(&self, context: &zmq::Context) -> Result<(), EngineError> {
        let socket_data = SocketData::default();
        // wait for a message from all plugins
        let total_plugins = self.plugins.len();
        let sync_socket = context
            .socket(zmq::REP)
            .map_err(|_e| EngineError::EngineSyncSocketCreateError())?;
        let sync_tcp_url = format!("tcp://*{}", socket_data.sync_socket_port);
        // here, we bind to BOTH the tcp and inproc endpoints, since plugins could be syncing on other.
        sync_socket.bind(&sync_tcp_url).map_err(|_e| {
            EngineError::EngineSyncSocketTCPBindError(socket_data.sync_socket_port)
        })?;
        sync_socket
            .bind(&socket_data.sync_inproc_url)
            .map_err(|_e| {
                EngineError::EngineSyncSocketInprocBindError(socket_data.sync_inproc_url)
            })?;

        let mut ready_plugins = 0;
        while ready_plugins < total_plugins {
            // receive message from plugin
            let _msg = sync_socket
                .recv_msg(0)
                .map_err(|_e| EngineError::EngineSyncSocketMsgRcvError())?;
            ready_plugins += 1;
        }
        // send a reply to all plugins
        let mut msg_sent = 0;
        while msg_sent < total_plugins {
            let reply = "ok";
            sync_socket
                .send(reply, 0)
                .map_err(|_e| EngineError::EngineSyncSocketSendRcvError())?;
            msg_sent += 1;
        }

        Ok(())
    }

    fn start_plugins(&'static self) -> Result<(), EngineError> {
        // call start_plugin with the zmq context and the config for each plugin,
        // as defined in the PLUGINS constant
        for plugin in &self.plugins {
            self.start_plugin(&self.context, plugin)?;
        }
        // once all plugins have been started, sync them with individual messages on the
        // REQ-REP sockets
        self.sync_plugins(&self.context)?;
        Ok(())
    }

    pub fn register_plugin(mut self, plugin: Arc<Mutex<Box<dyn Plugin>>>) -> Self {
        self.plugins.push(plugin);
        self
    }

    pub fn run(&'static mut self) -> Result<&Self, EngineError> {
        // incoming and outgoing sockets for the engine
        let sub_socket = self.get_sub_socket()?;
        let pub_socket = self.get_pub_socket()?;

        // start plugins in their own thread
        self.start_plugins()?;

        let _result = zmq::proxy(&pub_socket, &sub_socket)
            .expect("Engine got error running proxy; socket was closed?");

        Ok(self)
    }
}

#[cfg(test)]
mod tests {

    use std::{
        str,
        sync::{Arc, Mutex},
        vec,
    };

    use crate::{
        events::{Event, EventType},
        plugins::Plugin,
        App,
    };

    // Here we provide two simple, example event types. TypeA, which has a single string field, and TypeB which has a single
    // integer field.
    struct TypeAEventType {}
    impl EventType for TypeAEventType {
        fn get_name(&self) -> &'static str {
            let s = "TypeA";
            s
        }

        fn get_filter(&self) -> Result<&'static [u8], crate::errors::EngineError> {
            // just return the bytes associated with the name.
            Ok(self.get_name().as_bytes())
        }
    }

    // Example event for event type TypeA
    struct TypeAEvent {
        message: String,
    }
    impl TypeAEvent {
        fn from_bytes(mut b: Vec<u8>) -> TypeAEvent {
            // remove the first 5 bytes which are the message type
            for _i in 1..5 {
                b.remove(0);
            }
            let msg = str::from_utf8(&b).unwrap();
            TypeAEvent {
                message: msg.to_string(),
            }
        }
    }

    impl Event for TypeAEvent {
        fn to_bytes(&self) -> Result<Vec<u8>, crate::errors::EngineError> {
            let type_a = TypeAEventType {};
            // The byte array begins with the filter and then adds the message
            let result = [type_a.get_filter().unwrap(), self.message.as_bytes()].concat();
            Ok(result)
        }
    }

    // The second event type, TypeB.
    struct TypeBEventType {}
    impl EventType for TypeBEventType {
        fn get_name(&self) -> &'static str {
            let s = "TypeB";
            s
        }

        fn get_filter(&self) -> Result<&'static [u8], crate::errors::EngineError> {
            // just return the bytes associated with the name.
            Ok(self.get_name().as_bytes())
        }
    }

    // Event for event type TypeB
    struct TypeBEvent {
        count: usize,
    }
    impl TypeBEvent {
        fn from_bytes(mut b: Vec<u8>) -> TypeBEvent {
            // remove the first 5 bytes which are the message type
            for _i in 1..5 {
                b.remove(0);
            }
            let msg = str::from_utf8(&b).unwrap();
            TypeBEvent {
                count: msg.to_string().parse().unwrap(),
            }
        }
    }

    impl Event for TypeBEvent {
        fn to_bytes(&self) -> Result<Vec<u8>, crate::errors::EngineError> {
            // this is a TypeB event
            let type_b = TypeBEventType {};
            let message = format!("{}", self.count);
            // The byte array begins with the filter and then adds the message
            let result = [type_b.get_filter().unwrap(), message.as_bytes()].concat();
            Ok(result)
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
            pub_socket: zmq::Socket,
            sub_socket: zmq::Socket,
        ) -> Result<(), crate::errors::EngineError> {
            // send 5 messages
            let mut total_messages_sent = 0;
            while total_messages_sent < 5 {
                let message = format!("This is message {}", total_messages_sent);
                let m = TypeAEvent { message };
                let data = m.to_bytes().unwrap();
                pub_socket.send(data, 0).unwrap();
                total_messages_sent += 1;
            }

            // now get the TypeB events
            let mut total_messages_read = 0;
            while total_messages_read < 5 {
                // get the bytes of a new message; it should be of TypeB
                let b = sub_socket.recv_bytes(0).unwrap();
                let event_msg = TypeBEvent::from_bytes(b);
                let count = event_msg.count;
                println!("Got a type B message; count was: {}", count);
                total_messages_read += 1;
            }

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
            pub_socket: zmq::Socket,
            sub_socket: zmq::Socket,
        ) -> Result<(), crate::errors::EngineError> {
            // compute the counts of the first 5 messages
            let total_messages_read = 0;
            while total_messages_read < 5 {
                // get the bytes of a new message; it should be of TypeA
                let b = sub_socket.recv_bytes(0).unwrap();
                let event_msg = TypeAEvent::from_bytes(b);
                let count = event_msg.message.len();
                // send a TypeB event
                let m = TypeBEvent { count };
                let data = m.to_bytes().unwrap();
                pub_socket.send(data, 0).unwrap();
            }

            Ok(())
        }

        fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, crate::errors::EngineError> {
            Ok(vec![Box::new(TypeAEventType {})])
        }

        fn get_id(&self) -> uuid::Uuid {
            self.id
        }
    }

    #[test]
    fn test_run_app() {
        // the plugins for our app
        let msg_producer = MsgProducerPlugin::new();
        let counter = CounterPlugin::new();
        let app = App::new(5559, 5560);
        app.register_plugin(Arc::new(Mutex::new(Box::new(msg_producer))))
            .register_plugin(Arc::new(Mutex::new(Box::new(counter))));
        // runs forever...
        // app.run().unwrap();
    }
}