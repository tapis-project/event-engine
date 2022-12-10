
use serde::Deserialize;

use std::{env, fs};

use anyhow::{Result};

#[derive(Debug, Deserialize)]
pub enum MessageType {
    StringData,
    JSONData,
    // TODO -- eventually add support for Flatbuffers messages 
    // FlatbuffersData,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    // Type of messages that the MsgProducer should send 
    pub message_type: MessageType,
    // total number of messages to be sent by the MsgProducer
    pub num_messages: i32,
    // sizes of messages to send.
    // MsgProducer will vary messages between max and min sizes
    // For StringData, sizes indicate number of characters per message.
    // For JSON data, sizes indicate number of key/value pairs in the JSON object.
    pub min_message_size: i32,
    pub max_message_size: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self { message_type: MessageType::StringData, 
            num_messages: 1000, 
            min_message_size: 100, 
            max_message_size: 200 }
    }
}

// environment variable containing a path to the configuration file to use
const ENV_CONFIG_FILE_KEY : &str = "EE_LOAD_TEST_CONFIG_FILE";

// default configuration file to use 
const DEFAULT_CONFIG_FILE : &str = "~/load-test.toml";

// ---------------------------------------------------------------------------
// get_parms:
// ---------------------------------------------------------------------------
/** Retrieve the application parameters from the configuration file specified
 * either through an environment variable or as the first (and only) command
 * line argument.  If neither are provided, an attempt is made to use the
 * default file path.
 */
pub fn get_parms() -> Result<Parms> {
    // Get the config file path from the environment, command line or default.
    let config_file = env::var(ENV_CONFIG_FILE_KEY).unwrap_or_else(
        |_| {
            // Get the config file pathname as the first command line
            // parameter or use the default path.
            match env::args().nth(1) {
                Some(f) => f,
                None => DEFAULT_CONFIG_FILE.to_string()
            }
        });
    let contents = fs::read_to_string(&config_file)?;
    // Parse the toml configuration.
    let config : Config = toml::from_str(&contents)?; 
    Result::Ok(Parms { config_file: config_file, config})
}

// ***************************************************************************
//                                  Structs
// ***************************************************************************
// ---------------------------------------------------------------------------
// Parms:
// ---------------------------------------------------------------------------
#[derive(Debug)]
pub struct Parms {
    pub config_file: String,
    pub config: Config,
}