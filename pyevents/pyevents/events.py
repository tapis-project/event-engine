"""
A small library for developing event-engine plugins in Python. 

The basic usage is as follows:

import zmq
from events import get_plugin_socket, get_next_msg, publish_msg, send_quit_command

# create the zmq context object
context = zmq.Context()

# create the socket object your plugin will use to communicate with the engine;
# pass the port configured for your plugin
# optionally pass the host address of the engine if it is not available on the docker network
socket = get_plugin_socket(context, port)  

# main plugin loop
while True:
    # get the next message
    msg_bytes = get_next_msg(socket)
    
    # ... do something with the message ...
    
    # publish a message
    publish_msg(socket, some_bytes)

    if some_condition:
        break

# send a quit command when done
send_quit_command(socket)


"""
import zmq

def get_plugin_socket(context, port, host="engine"):
    """
    """
    # plugin socket utilizes REQ socket type
    plugin_socket = context.socket(zmq.REQ)
    plugin_socket_connect_str = f"tcp://{host}:{port}"    
    plugin_socket.connect(plugin_socket_connect_str)
    return plugin_socket


def get_next_msg(socket):
    """
    Get the next message for this plugin.
    Note that this function returns raw bytes and these bytes will typically need to be converted into the
    message format of your application.
    """
    print("top of get_next_msg; making request for next message")
    try:
        socket.send_string("plugin_command: next_msg")
    except Exception as e:
        print(f"Got exception trying to send next_msg command; details: {e}")
        raise e
    print("request made; waiting for reply")
    # wait for the reply
    data = socket.recv()
    print("got reply, returning data")
    return data


def publish_msg(socket, data):
    """
    Publish a new message.
    `data` should be a raw byte array
    Returns the reply from the engine.
    """
    try:
        socket.send(data)
    except Exception as e:
        print(f"Got exception trying to publish a new message; details: {e}")
        raise e
    # wait for the reply
    reply = socket.recv_string()
    if not "event-engine: msg published" in reply:
        print(f"unexpected response from engine when trying to publish a message; response: {reply}")
    return reply


def send_quit_command(socket):
    """
    Send a message to the engine that the this plugin is going to terminate execution.
    """
    try:
        socket.send_string("plugin_command: quit")
    except Exception as e:
        print(f"Got exception trying to send quit command; details: {e}")
        raise e
