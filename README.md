# Event Engine

A framework for writing event-based applications that utilize a plugin architecture. Applications built with `event-engine` are written in Rust but can utilize plugins in written in multiple languages. For the initial
release, support for external plugins in Python will be supported, but support can be added for additional 
languages if there is demand.

Events correspond to statically typed messages that can be transmitted over a socket and are the basic 
mechanism of communication between plugins of the application. Each application defines its own event types,
including the mechanisms for serializing and deserializing an event's message to/from bytes. No assumption 
is made about the serialization format used for events, and different event types can utilize 
different formats.

Plugins are independent components of an application that create and consume events. Each plugin defines
the event types it is interested in, and the core `event-engine` proxies event messages to the application's
plugins using a publish-subscribe pattern. 

Plugins can be "internal" or "external". Internal plugins run as child threads within the main (Rust) application process and are necessarily written in Rust. External plugins run as separate OS processes and 
can be written in any language. 

All event message communication happens via `zmq` sockets which `event-engine` manages. Internal plugins 
send and receive event messages using `inproc` 

![Depiction of an example application](./event_engine_design.png)


## Usage

The main steps to building an application using `event-engine` are as follows:

1. Define the `EventType`s and `Event`s for the application. The `events.rs` module includes two traits,
`EventType` and `Event`, to be implemented.
2. Define the plugins for the application. These are the indpendent components that will create and consume
events. The `plugins.rs` module includes the `Plugin` and `ExternalPlugin` traits to be implemented.
3. Create an `event_engine::App` object and configure it with the plugins for the application.

```
     // create the App object, register the plugins and run
     fn main() {
         // two Rust plugins
         let msg_producer = MsgProducerPlugin::new();
         let counter = CounterPlugin::new();
         // an external plugin written in python
         let pyplugin = PyPlugin::new();
     
         // main application object
         let app: App = App::new(5559, 5560);
         app.register_plugin(Arc::new(Box::new(msg_producer)))
             .register_plugin(Arc::new(Box::new(counter)))
             .register_external_plugin(Arc::new(Box::new(pyplugin)))
             .run()
             .unwrap();
         ()
     }
```

In the case of external plugins, a small Python module, `events.py`, included within the `pyevents` 
directory of this repository, has been written to simplify the process of writing Python plugins. 
There is also a Docker image, `tapis/pyevents`, which can be used to build a standalone container
with a Python plugin.


## Example

This repository includes a complete example application consisting of two (internal) Rust plugins and
one external Python plugin. The application is packaged as two Docker containers -- the main Rust 
application, with the two Rust plugins, runs in the first container and the Python plugin runs in the second
container. See the `example` directory for more details.

## 