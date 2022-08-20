# Event Engine

A framework for writing event-based applications that utilize a plugin architecture. Applications built with `event-engine` are written in Rust but can utilize plugins in written in multiple languages. FOr the initial
release, support for external plugins in Python will be supported, but adding support for addtional languages
should not be difficult.

Events correspond to messages with a specific type. 


![Depiction of an example application](./event_engine_design.png)

## Usage

