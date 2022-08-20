from operator import truediv
import time
import zmq
from events import get_plugin_socket, get_next_msg, publish_msg, send_quit_command

# create the zmq context object
context = zmq.Context()

# create the socket object your plugin will use to communicate with the engine;
# pass the port configured for your plugin
# optionally pass the host address of the engine if it is not available on the docker network
PYPLUGIN_TCP_PORT = 6000
socket = get_plugin_socket(context, PYPLUGIN_TCP_PORT)  

# main plugin loop
done = False
total_messages = 1
print("top of pyplugin, entering main loop")
while not done:
    # get the next message
    print(f"waiting on message: {total_messages}")
    msg_bytes = get_next_msg(socket)
    print(f"just got message {total_messages}; contents: {msg_bytes}")
    total_messages += 1
    
    if total_messages == 11:
        done = True


# publish a "TypeC" message
# sleep 0.3 seconds first to guarantee that all typeB events are received first.
time.sleep(0.3)
message = "TypeCGoodbye from python"
publish_msg(socket, message.encode())


# send a quit command when done
print("sending quit command...")
send_quit_command(socket)
print("quit command sent; exiting.")
