use std::sync::Arc;
/// Event-engine load program
/// This program creates a load test from a configuration file for the event-engine library.
/// The load test currently exercises three plugins.
/// Plugin A -- MessageProducer -- generates messages of different types
/// PluginB -- MessageOperator -- receives PluginA messages and performs some operation on the 
///         message (e.g., character count)
/// PluginC -- MessageAccumulator -- collects all messages from B and accumulates a total.
/// 
/// Ultimately, the message type will be configurable, with options such as "String", "JSON", and
/// "flatbufffers". Additionally, the number of messages to send as well as the "size" of each message
/// can be configured, to stress the performance of event-engine in different ways.
use std::{str};
use event_engine::App;
use log::{debug, info};
use uuid::{self, Uuid};
use zmq::Socket;
use lazy_static::lazy_static;
use event_engine::plugins::{Plugin};
use event_engine::events::{Event, EventType};
use event_engine::errors::EngineError;


mod config;

struct InitialMsgStringEventType {}
impl EventType for InitialMsgStringEventType {
    fn get_name(&self) -> String {
        let s = "InitialMsgString";
        s.to_string()
    }

    fn get_filter(&self) -> Result<Vec<u8>, EngineError> {
        // just return the bytes associated with the name.
        Ok(self.get_name().as_bytes().to_vec())
    }
}

struct InitialMsgString {
    message: String,
}
impl Event for InitialMsgString {
    fn to_bytes(&self) -> Result<Vec<u8>, EngineError> {
        let msg_type = InitialMsgStringEventType {};
        // The byte array begins with the filter and then adds the message
        let result = [
            msg_type.get_filter().unwrap(),
            self.message.as_bytes().to_vec(),
        ]
        .concat();
        Ok(result)
    }

    fn from_bytes(mut b: Vec<u8>) -> Result<InitialMsgString, Box<(dyn std::error::Error + 'static)>> {
        // remove the first 16 bytes which are the message type
        let msg_type = InitialMsgStringEventType {};
        let msg_filter_len = msg_type.get_filter().unwrap().len();
        for _i in 0..msg_filter_len {
            b.remove(0);
        }
        let msg = str::from_utf8(&b).unwrap();
        Ok(InitialMsgString {
            message: msg.to_string(),
        })
    }
}

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
    fn get_name(&self) -> String {
        "MsgProducerPlugin".to_string()
    }

    fn start(
        &self,
        pub_socket: Socket,
        _sub_socket: Socket,
    ) -> Result<(), EngineError> {
        info!(
            "MsgProducer (plugin id {}) start function starting...",
            self.get_id()
        );

        // send PARMS.config.num_messages messages
        let mut total_messages_sent = 0;
        let mut current_size_offset = 0;
        while total_messages_sent < PARMS.config.num_messages {
            // Generate a message of the right size; we round-robin between the minimum and 
            // maximum sizes, incrementing by one for each message until we hit the max, then
            // we start back over at the minimum. 
            let msg_size = PARMS.config.min_message_size + current_size_offset;
            current_size_offset += 1;
            if current_size_offset > PARMS.config.max_message_size - PARMS.config.min_message_size {
                current_size_offset = 0;
            }
            // the "message" will be a String consisting of a repeated character ("X"), repeated a number of 
            // times equal to 
            let message = (0..msg_size).map(|_| "X").collect::<String>();
            let m = InitialMsgString { message };
            let data = m.to_bytes().unwrap();
            debug!("MsgProducer sending bytes: {:?}", data);
            pub_socket.send(data, 0).unwrap();
            total_messages_sent += 1;
            debug!(
                "MsgProducer sent InitialMsgString event message #{}",
                total_messages_sent
            );
        }
        info!(
            "MsgProducer has sent all InitialMsgString event messages, now exiting..."
        );

        Ok(())
    }

    // the MsgProducer plugin does not subscribe to any messages
    fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, EngineError> {
        Ok(vec![])
    }

    fn get_id(&self) -> uuid::Uuid {
        self.id
    }
}

struct OperatorMsgEventType{}
impl EventType for OperatorMsgEventType {
    fn get_name(&self) -> String {
        let s = "OperatorMsg";
        s.to_string()
    }

    fn get_filter(&self) -> Result<Vec<u8>, EngineError> {
        // just return the bytes associated with the name.
        Ok(self.get_name().as_bytes().to_vec())
    }
}

struct OperatorMsg {
    count: String,
}

impl Event for OperatorMsg {
    fn to_bytes(&self) -> Result<Vec<u8>, EngineError> {
        let msg_type = OperatorMsgEventType {};
        let result = [
            msg_type.get_filter().unwrap(),
            self.count.as_bytes().to_vec(),
        ]
        .concat();
        Ok(result)
    }

    fn from_bytes(mut b: Vec<u8>) -> Result<OperatorMsg, Box<dyn std::error::Error>> {

        let msg_type = OperatorMsgEventType {};
        let msg_filter_len = msg_type.get_filter().unwrap().len();
        for _i in 0..msg_filter_len {
            b.remove(0);
        }
        let count = str::from_utf8(&b).unwrap();
        
        Ok(OperatorMsg {
            count: count.to_string(),
        })
    }
}

struct MsgOperatorPlugin {
    id: uuid::Uuid,
}
impl MsgOperatorPlugin {
    fn new() -> Self {
        MsgOperatorPlugin {
            id: uuid::Uuid::new_v4(),
        }
    }
}

impl Plugin for MsgOperatorPlugin {
    fn start(&self, pub_socket: Socket, sub_socket: Socket) -> Result<(), EngineError> {
        let mut total_messages_read = 0;
        while total_messages_read <  PARMS.config.num_messages {
            // get the bytes of a new message; it should be of TypeA
            let b = sub_socket.recv_bytes(0).unwrap();
            total_messages_read += 1;
            let event_msg = InitialMsgString::from_bytes(b).unwrap();
            // count the length of the message
            let count = event_msg.message.len();
            // send a new OperatorMsg with the count
            let m = OperatorMsg { count: count.to_string() };
            let data = m.to_bytes().unwrap();
            pub_socket.send(data, 0).unwrap();
        }

        Ok(())

    }

    fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, EngineError> {
        Ok(vec![Box::new(InitialMsgStringEventType {})])
    }

    fn get_id(&self) -> Uuid {
        self.id
    }

    fn get_name(&self) -> String {
        "MsgOperatorPlugin".to_string()
    }
}

// The accumulator plugin; this plugin does not send any messages. It subscribes to the OperatorMsg
// and computes a total count from the individual counts.
struct AccumulatorPlugin {
    id: uuid::Uuid,
}
impl AccumulatorPlugin {
    fn new() -> Self {
        AccumulatorPlugin {
            id: uuid::Uuid::new_v4(),
        }
    }
}
impl Plugin for AccumulatorPlugin {
    fn start(&self, _pub_socket: Socket, sub_socket: Socket) -> Result<(), EngineError> {
        let mut total_messages_read = 0;
        let mut total_count = 0;
        while total_messages_read <  PARMS.config.num_messages {
            // get the bytes of a new message; it should be of type OperatorMsg
            let b = sub_socket.recv_bytes(0).unwrap();
            let operator_msg = OperatorMsg::from_bytes(b).unwrap();
            let count = operator_msg.count;
            // The convertion to an i32 could fail if the value passed in the message is not
            // an integer. In this case, we ignore it entirely. 
            if let Ok(count_int) = count.parse::<i32>() {
                total_count += count_int;
                total_messages_read += 1;
            }
            
        }
        println!("Total messages read by accumulator: {}", total_messages_read);
        println!("Total count for entire program: {}", total_count);

        Ok(())
    }

    fn get_subscriptions(&self) -> Result<Vec<Box<dyn EventType>>, EngineError> {
        Ok(vec![Box::new(OperatorMsgEventType {})])
    }

    fn get_id(&self) -> Uuid {
        self.id
    }

    fn get_name(&self) -> String {
        "AccumulatorPlugin".to_string()
    }
}



// ***************************************************************************
//                             Static Variables 
// ***************************************************************************
// Lazily initialize the parameters variable so that is has a 'static lifetime.
// We exit if we can't read our parameters.
lazy_static! {
    static ref PARMS: config::Parms = config::get_parms().unwrap();
}

fn main() {

    // configure logging
    // log4rs::init_file(LOG4RS_CONFIG_FILE, Default::default()).expect("Could not configure logger, quitting!");
    // // the plugins for our app
    // info!("test_run_app starting");
    let msg_producer = MsgProducerPlugin::new();
    let operator_plugin = MsgOperatorPlugin::new();
    let accumulator_plugin = AccumulatorPlugin::new();
    let app: App = App::new(5559, 5560);
    app.register_plugin(Arc::new(Box::new(msg_producer)))
        .register_plugin(Arc::new(Box::new(operator_plugin)))
        .register_plugin(Arc::new(Box::new(accumulator_plugin)))
        .run()
        .map_err(|e| format!("Got error from Engine! Details: {}", e))
        .unwrap();
    info!("returned from test_run_app.run()");
    ()
}
