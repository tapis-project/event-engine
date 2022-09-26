# Example Application

This directory contains a sample application written using the `event-engine` crate and
`pyevents` library. It contains three plugins, two (internal) Rust plugins and one 
external Python plugin.

The example application requires the `event-engine` library to build as well as the associated Docker images.
These can all be built with a single `make build` command.

Once all images have been built, run the application with `make up`. This uses docker-compose under the hood to start
up the two containers comprising the example application. One container runs the primary Rust application, and the other
container runs the external Python plugin process.

If the application runs correctly, you should see a number of messages printed to the screen, and then the program should
exit. The final log messages should look something like:

```
pyplugin exited with code 0
engine exited with code 0
```


