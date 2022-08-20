use std::{str, sync::Arc};
use uuid::{self, Uuid};
use zmq::Socket;
use event_engine::App;
use event_engine::plugins::{Plugin, ExternalPlugin};
use event_engine::events::{Event, EventType};
use event_engine::errors::EngineError;

// Here we provide two simple, example event types. TypeA, which has a single string field,
// and TypeB which has a single integer field.
struct TypeAEventType {}
impl EventType for TypeAEventType {
    fn get_name(&self) -> String {
        let s = "TypeA";
        s.to_string()
    }

    fn get_filter(&self) -> Result<Vec<u8>, EngineError> {
        // just return the bytes associated with the name.
        Ok(self.get_name().as_bytes().to_vec())
    }
}

// Example event for event type TypeA
struct TypeAEvent {
    message: String,
}

impl Event for TypeAEvent {
    fn to_bytes(&self) -> Result<Vec<u8>, EngineError> {
        let type_a = TypeAEventType {};
        // The byte array begins with the filter and then adds the message
        let result = [
            type_a.get_filter().unwrap(),
            self.message.as_bytes().to_vec(),
        ]
        .concat();
        Ok(result)
    }

    fn from_bytes(mut b: Vec<u8>) -> Result<TypeAEvent, EngineError> {
        // remove the first 5 bytes which are the message type
        for _i in 0..5 {
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

    fn get_filter(&self) -> Result<Vec<u8>, EngineError> {
        // just return the bytes associated with the name.
        Ok(self.get_name().as_bytes().to_vec())
    }
}

// Event for event type TypeB
struct TypeBEvent {
    count: usize,
}

impl Event for TypeBEvent {
    fn to_bytes(&self) -> Result<Vec<u8>, EngineError> {
        // this is a TypeB event
        let type_b = TypeBEventType {};
        let message = format!("{}", self.count);
        // The byte array begins with the filter and then adds the message
        let result = [type_b.get_filter().unwrap(), message.as_bytes().to_vec()].concat();
        Ok(result)
    }

    fn from_bytes(mut b: Vec<u8>) -> Result<TypeBEvent, EngineError> {
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

struct TypeCEventType {}
impl EventType for TypeCEventType {
    fn get_name(&self) -> String {
        let s = "TypeC";
        s.to_string()
    }

    fn get_filter(&self) -> Result<Vec<u8>, EngineError> {
        // just return the bytes associated with the name.
        Ok(self.get_name().as_bytes().to_vec())
    }
}

struct TypeCEvent {
    message: String,
}

impl Event for TypeCEvent {
    fn to_bytes(&self) -> Result<Vec<u8>, EngineError> {
        let type_c = TypeCEventType {};
        // The byte array begins with the filter and then adds the message
        let result = [
            type_c.get_filter().unwrap(),
            self.message.as_bytes().to_vec(),
        ]
        .concat();
        Ok(result)
    }

    fn from_bytes(mut b: Vec<u8>) -> Result<TypeCEvent, EngineError> {
        // remove the first 5 bytes which are the message type
        for _i in 0..5 {
            b.remove(0);
        }
        let msg = str::from_utf8(&b).unwrap();
        Ok(TypeCEvent {
            message: msg.to_string(),
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
    ) -> Result<(), EngineError> {
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
        println!(
            "MsgProducer has sent all TypeA event messages, now waiting to receive TypeB events"
        );

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
        println!("MsgProducer has received all TypeB event messages");

        // finally, wait for the TypeC event from the python plugin
        let b = sub_socket.recv_bytes(0).unwrap();
        println!("MsgProducer received last message; bytes: {:?}", b);
        let event_msg = TypeCEvent::from_bytes(b).unwrap();
        println!("MsgProduce got message: {}", event_msg.message);
        
        println!("MsgProducer has now received all events; exting...");

        Ok(())
    }

    fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, EngineError> {
        Ok(vec![Box::new(TypeBEventType {}), Box::new(TypeCEventType {})])
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
    ) -> Result<(), EngineError> {
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

    fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, EngineError> {
        Ok(vec![Box::new(TypeAEventType {})])
    }

    fn get_id(&self) -> uuid::Uuid {
        self.id
    }
}

struct PyPlugin {
    id: uuid::Uuid,
}

impl PyPlugin{
    fn new() -> Self {
        PyPlugin {
            id: uuid::Uuid::new_v4(),
        }
    }
}

impl ExternalPlugin for PyPlugin {
    fn get_tcp_port(&self) -> i32 {
        6000
    }

    fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, EngineError> {
        Ok(vec![Box::new(TypeAEventType {}), Box::new(TypeBEventType {})])
    }

    fn get_id(&self) -> Uuid {
        self.id
    }
}

fn main() {
    // the plugins for our app
    println!("inside the test_run_app");
    let msg_producer = MsgProducerPlugin::new();
    let counter = CounterPlugin::new();
    let pyplugin = PyPlugin::new();
    println!("plugins for test_run_app configured");
    let app: App = App::new(5559, 5560);
    app.register_plugin(Arc::new(Box::new(msg_producer)))
        .register_plugin(Arc::new(Box::new(counter)))
        .register_external_plugin(Arc::new(Box::new(pyplugin)))
        .run()
        .map_err(|e| format!("Got error from Engine! Details: {}", e))
        .unwrap();
    println!("returned from test_run_app.run()");
    ()
}
