use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    /// received by the client when joining the master server.
    /// 
    /// id is the unique id assigned to the client.
    /// other clients will only know about this client if the client broadcasts this information
    Welcome {
        id:Uuid
    },
    /// received by both clients and the server
    /// sent by a client and distributed to other clients to let them know about 'self'.
    /// info contains client provided information, e.g. name of a multiplayer room name, number of players, etc, which is application dependend.  
    Broadcast {
        id:Uuid,
        info:String
    }
}

impl Protocol {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    pub fn from_json(json:&str) -> Result<Self, String> {
        match serde_json::from_str(json) {
            Ok(msg) => Ok(msg),
            Err(err) => Err(err.to_string()),
        }
    }
}