/**************************************************************************\
* Cavalier: extralive chat                                                 *
* Copyright (C) 2025 Samuel A. Mansfield                                   *
*                                                                          *
* This program is free software: you can redistribute it and/or modify     *
* it under the terms of the GNU Affero General Public License as           *
* published by the Free Software Foundation, either version 3 of the       *
* License, or (at your option) any later version.                          *
*                                                                          *
* This program is distributed in the hope that it will be useful,          *
* but WITHOUT ANY WARRANTY; without even the implied warranty of           *
* MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the            *
* GNU Affero General Public License for more details.                      *
*                                                                          *
* You should have received a copy of the GNU Affero General Public License *
* along with this program.  If not, see <https://www.gnu.org/licenses/>.   *
\**************************************************************************/

//! Axum backend for cavalier
//!
//!

use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{self, WebSocket},
    },
    response::Response,
    routing::{any, get},
};
use serde::{Deserialize, Serialize};
// use serde_json::Result;
use tokio::sync::broadcast::{self, Receiver, Sender};

/**********************\
* Main, Routing, State *
\**********************/

#[derive(Clone)]
struct AppState {
    ws_tx: Sender<MessageUpdate>,
    messages: Vec<Message>,
    /* db connection */
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:80")
        .await
        .expect("Could not bind to 0.0.0.0:80");
    println!("Listening on {:?}", listener.local_addr().unwrap());
    println!("Waiting for connection");
    let (tx, _) = broadcast::channel(69);

    let msgs = Vec::<Message>::from([Message {
        id: 0,
        text: String::from("Default starting message. Sneed's Feed and Seed."),
    }]);

    let state = AppState {
        ws_tx: tx,
        messages: msgs,
    };

    let router = Router::new()
        .route("/ws-events", any(events_handler)) // client <-> server event communication
        .route("/ws-key", any(key_handler)) // client <-> server keystrokes communication
        .route("/get-test", get(get_test_handler))
        .with_state(state);
    axum::serve(listener, router).await.unwrap()
}

async fn get_test_handler(state: State<AppState>) -> axum::response::Html<&'static str> {
    axum::response::Html("<h1 style=\"text-align: center;\">GET test</h1>")
}

async fn events_handler(ws: WebSocketUpgrade, state: State<AppState>) -> Response {
    ws.on_upgrade(|ws| ws_events_handler(ws, state))
}

async fn key_handler(ws: WebSocketUpgrade, state: State<AppState>) -> Response {
    ws.on_upgrade(|ws| ws_events_handler(ws, state))
}

/************\
* Event Code *
\************/

/// The base event sent and received from websockets
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Event {
    t: EventType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum EventType {
    MessageNew,
    MessageUpdate,
    MessageEnd,
}

/// A completed Message.
/// The text contains all keystrokes, including backspace.
/// Timings will be added
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Message {
    id: i64,
    text: String,
}

/// A message update
/// This is how the client sends keystroke(s) to the server
#[derive(Serialize, Deserialize, Debug, Clone)]
struct MessageUpdate {
    message_id: i64,
    keystrokes: Vec<KeyStroke>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyStroke {
    pub key: char,
    /* time: std::time::Duration, */  //this will get added in when DB functionality is added
}

async fn ws_events_handler(mut ws: WebSocket, State(state): State<AppState>) {
    ws.send(ws::Message::Ping("A".into())).await.unwrap();
}

/**********************************\
* Client <-> Server Keystroke Code *
\**********************************/
// TODO: See this webside for how to split the websocket send/recv
// https://github.com/tokio-rs/axum/blob/main/examples/websockets/src/main.rs

/// capture keystrokes
/// send message updates down the tokio tx
async fn ws_c2s_task(mut ws: WebSocket, State(state): State<AppState>) {
    let tx = state.ws_tx.clone();
    while let Some(Ok(msg)) = ws.recv().await {
        if let ws::Message::Text(msg) = msg {
            let keystroke: KeyStroke =
                serde_json::from_str(msg.as_str()).unwrap_or(KeyStroke { key: '\0' });
            println!("{:?}", keystroke);

            let keyvec = Vec::<KeyStroke>::from([keystroke]);
            let msg_update = MessageUpdate {
                message_id: 0,
                keystrokes: keyvec,
            };
            let _ = tx.send(msg_update).map_err(|e| {
                eprintln!("Internal transmission failed: {e}");
            });
        }
    }
}

/***********************\
* Client -> Server Code *
\***********************/

/// broadcast keystokes
/// receive message update from the tokio tx
async fn ws_s2c_handler(mut ws: WebSocket, State(state): State<AppState>) {
    let mut rx = state.ws_tx.subscribe();
    loop {
        let msg_update = rx
            .recv()
            .await
            .map_err(|e| {
                eprintln!("Receive error: {e}");
            })
            .unwrap();
        let json = serde_json::to_string(&msg_update).unwrap();
        let ws_msg = ws::Message::Text(json.into());
        ws.send(ws_msg).await.unwrap();
    }
}
