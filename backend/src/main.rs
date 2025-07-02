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
//! This backend provides three endpoints:
//! 1. `/api/ws/events/`: A websocket for sending `Event`s (server -> client)
//! 2. `/api/ws/key`: A websocket for sending keystrokes as binary arrays (server <-> client)
//! 3. `/apt/msg/*`: JSON APIs for getting message data (server -> client)
//!
// TODO: instead of using expect() / panicing during websocket threads, end the loop and send
// ws::Message::Close to terminate the connection gracefully. Also program behavior for when
// ws::Message::Close is received from the socket to exit gracefully.

// TODO: refactor application
// Ideas: since there is a global state, all routes accessing that state can go in their own
// location. Since I use tokio::broadcast for nice encapsulated communication, almost every handler
// and component can be out of scope from one another. Therefore, the best way to break this app
// down is by functionality. The websocket handlers go on their own, the message JSON apis go on
// their own, the future DB code goes on its own, and the future account system and message
// deleting/moderating goes all on its own.
//  However, one massive consideration is the TIMING code. I want each message to have timing
// information stored, which will enable playback of messages. This code will change the Message
// struct, and it will affect the websocket handlers, and database. Otherwise, it could just be
// fine.

use axum::{
    Json, Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{self, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{any, get},
};
use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
// use serde_json::Result;
use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use std::collections::HashMap;
use tokio::sync::{
    RwLock,
    broadcast::{self, Sender},
};
use tower_sessions::{Expiry, MemoryStore, Session, SessionManagerLayer, session::Id as SessionId};

/***********************\
* Global Structs, Enums *
\***********************/
// TODO: refactor: move these into shared crate

/// A completed Message.
///
/// The text contains all keystrokes, including backspace.
/// Timings will be added
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Message {
    id: u32,
    text: String,
}

/// A keystroke
///
/// Associates key char with message_id, timing, any other info.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Keystroke {
    message_id: u32,
    key: char,
    /* time: std::time::Duration, */  //this will get added in when DB functionality is added
}

/// An event
///
/// Communicates from the server to the client that a new message has been created, a message is
/// over, there is a new user, etc any live updates the client could want
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "event", content = "data")]
enum Event {
    MessageNew(Message),
    MessageEnd,
}

/**********************\
* Main, Routing, State *
\**********************/

#[derive(Clone)]
struct AppState {
    key_tx: Sender<Keystroke>,
    event_tx: Sender<Event>,
    messages: Arc<RwLock<Vec<Message>>>,
    session_to_message: Arc<RwLock<HashMap<SessionId, u32>>>,
    /* db connection */
}

#[tokio::main]
async fn main() {
    #[cfg(not(debug_assertions))]
    const BASE_URL: &'static str = "0.0.0.0:80";
    #[cfg(debug_assertions)]
    const BASE_URL: &'static str = "127.0.0.1:3000";

    let listener = tokio::net::TcpListener::bind(BASE_URL)
        .await
        .expect(format!("Could not bind to {BASE_URL}").as_str());
    println!("Listening on {:?}", listener.local_addr().unwrap());

    let (key_tx, _) = broadcast::channel(10_000); // Keystroke tx
    let (event_tx, _) = broadcast::channel(10_000); // event tx

    let mut msgvec = Vec::<Message>::from([Message {
        id: 0,
        text: String::from(
            "Hello! Welcome to Cavalier Extralive Chat. As you type your message, it will reflect to your friends in real time. No prose, just rash and cavalier messages! All messages are anonymous and stored in RAM, thus they are securely deleted when the server restarts. This project is provided to you under the GNU AGPL3. To see the source code of this app, visit https://github.com/samfield1/cavalier/ 🫠 你们玩儿",
        ),
    }]);
    msgvec.reserve(10);
    let messages = Arc::new(RwLock::new(msgvec));

    let session_to_message_map = HashMap::<SessionId, u32>::new();
    let session_to_message = Arc::new(RwLock::new(session_to_message_map));

    let state = AppState {
        key_tx,
        event_tx,
        messages,
        session_to_message,
    };

    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(true)
        .with_expiry(Expiry::OnInactivity(time::Duration::minutes(10)))
        .with_path("/api");

    let router = Router::new()
        .route("/api/ws/events", any(events_handler)) // client <-> server event communication
        .route("/api/ws/key", any(key_handler)) // client <-> server keystrokes communication
        .route("/api/msg/new", any(msg_new_handler)) // json API: writing new message
        .route("/api/msg/get", get(msg_get_handler)) // json API: get existing messages
        .route("/api/session/new", any(session_new_handler)) // associate user with new session
        .route("/api/test", any(test_handler)) // test if axum is running
        .layer(session_layer)
        .with_state(state);
    axum::serve(listener, router).await.unwrap()
}

async fn test_handler(_: State<AppState>) -> axum::response::Html<&'static str> {
    axum::response::Html("<h1 style=\"text-align: center;\">GET test</h1>")
}

/*******************\
* Message JSON APIs *
\*******************/

#[axum::debug_handler]
async fn msg_new_handler(State(state): State<AppState>, session: Session) -> impl IntoResponse {
    session.insert("preserve", true).await.unwrap(); // ensures session
    let msgs_ref = state.messages.clone();
    let event_tx = state.event_tx.clone();
    let msg_id: u32;
    let new_msg: Message;
    // add to global state vec
    // inside scope to drop guard at end
    if let Ok(mut msgs) = msgs_ref.try_write() {
        msg_id = u32::try_from(msgs.len()).expect("Message id u32 overflow!");
        new_msg = Message {
            id: msg_id,
            text: String::new(),
        };
        (*msgs).push(new_msg.clone());
    } else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json("Could not access messages, try again."),
        )
            .into_response();
    };

    // TODO: the session check must go above the new message allocation
    // add to global session RwLock
    match session.id() {
        Some(session_id) => {
            let session_to_message_ref = state.session_to_message.clone();
            let mut session_to_message = (*session_to_message_ref).write().await;
            session_to_message.insert(session_id, msg_id);
        }
        None => {
            eprintln!("Session: {:?}", session);
            return (
                StatusCode::BAD_REQUEST,
                Json("Session must be set to make a new message"),
            )
                .into_response();
        }
    }
    // This is not used, because I need the message_id to update while the websocket threads are
    // running. Therefore, the session is just assosciated with an Arc<RwLock<HashMap<>>> inside
    // the global app state struct
    // match session.insert("message_id", msg_id).await {
    //     Ok(_) => {}
    //     Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string() at)).into_response(),
    // }

    // transmit new message event
    let new_msg_event = Event::MessageNew(new_msg.clone());
    if let Err(e) = event_tx.send(new_msg_event) {
        eprintln!("Error broadcasting new message: {e}")
    }
    (StatusCode::OK, Json(new_msg)).into_response()
}

async fn msg_get_handler(State(state): State<AppState>, session: Session) -> impl IntoResponse {
    session.insert("preserve", true).await.unwrap();
    let msgs = state.messages.clone();
    let Ok(msgs) = msgs.try_read() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json("Could not access messages, try again."),
        )
            .into_response();
    };

    (StatusCode::OK, Json(&*msgs)).into_response()
}

/************\
* Event Code *
\************/

async fn events_handler(ws: WebSocketUpgrade, state: State<AppState>) -> Response {
    ws.on_upgrade(|ws| ws_events_handler(ws, state))
}

/// Send updates to the client live as `Event` jsons
async fn ws_events_handler(ws: WebSocket, State(state): State<AppState>) {
    let mut event_rx = state.event_tx.subscribe();

    let (mut sender, mut receiver) = ws.split();

    // Always read from the socket to keep it alive
    tokio::spawn(async move {
        // TODO: think about updating both websocket handlers to deal with close and pingpong. However, recognize the
        // limited utility, because the main thread's send loop has no way to access this
        // information without an Arc<Mutex<>>
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                ws::Message::Close(_) => {
                    dbg!("/api/ws/events/: received Close");
                    break;
                }
                _msg => {}
            }
        }
    });

    // Send events
    while let Ok(event) = event_rx.recv().await {
        let json = serde_json::to_string(&event).unwrap();
        let ws_msg = ws::Message::Text(json.into());
        if let Err(e) = sender.send(ws_msg).await {
            eprintln!("Event send error: {e}");
            break;
        }
    }
}

/**********************************\
* Client <-> Server Keystroke Code *
\**********************************/

async fn key_handler(ws: WebSocketUpgrade, state: State<AppState>, session: Session) -> Response {
    session.insert("preserve", true).await.unwrap(); // ensures session
    if session.id().is_none() {
        return (StatusCode::BAD_REQUEST, "Session id is not set!").into_response();
    }
    ws.on_upgrade(|ws| ws_key_handler(ws, state, session))
}

/// Split websocket into send/recv and propogate to separate client -> server and server -> client
/// functions
async fn ws_key_handler(ws: WebSocket, state: State<AppState>, session: Session) {
    let (ws_tx, ws_rx) = ws.split();
    let mut s2c_task = tokio::spawn(ws_s2c_task(ws_tx, state.clone(), session.clone()));
    let mut c2s_task = tokio::spawn(ws_c2s_task(ws_rx, state, session));
    tokio::select! {
        s2c_result = (&mut s2c_task) => {
            match s2c_result {
                Ok(_) => {},
                Err(e) => eprintln!("Error sending keystrokes: {e}")
            }
            c2s_task.abort();
        },
        c2s_result = (&mut c2s_task) => {
            match c2s_result {
                Ok(_) => {},
                Err(e) => eprintln!("Error receiving keystrokes {e}")
            }
            s2c_task.abort();
        }
    }

    /***********************\
    * Server -> Client Code *
    \***********************/

    /// receive keystrokes from the key_rx
    /// send keystrokes to all clients but the originator
    async fn ws_s2c_task(
        mut ws_tx: SplitSink<WebSocket, ws::Message>,
        State(state): State<AppState>,
        _session: Session,
    ) {
        let mut rx = state.key_tx.subscribe();
        loop {
            match rx.recv().await {
                Ok(keystroke) => {
                    // don't broadcast if the keystroke came from this session
                    // TODO: reevaluate if this is useful. It is turned off now for two reasons:
                    // 1. easier to debug
                    // 2. The client only echoing the character when the server responds gives the
                    //    user hangup when lagging instead of false feedback
                    // let self_msg: bool;
                    // {
                    //     let session_id = session
                    //         .id()
                    //         .expect("ws key_tx task started without session id");
                    //     let session_to_message_ref = state.session_to_message.clone();
                    //     let session_to_message = (*session_to_message_ref).read().await;
                    //     self_msg = match session_to_message.get(&session_id) {
                    //         Some(message_id) => *message_id == keystroke.message_id,
                    //         None => false,
                    //     }
                    //     // then only broadcast if self_msg == false
                    // }

                    // build buffer.
                    // first 4 bytes = little endian char/key
                    // last 4 bytes = little endian message id
                    let mut buffer = BytesMut::with_capacity(8);
                    buffer.put_u32_le(keystroke.key as u32);
                    buffer.put_u32_le(keystroke.message_id);
                    let msg_bytes = buffer.freeze();
                    let ws_msg = ws::Message::Binary(msg_bytes);
                    if let Err(e) = ws_tx.send(ws_msg).await {
                        eprintln!("Error sending ws_tx: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("Error receiving key_rx: {e}");
                }
            }
        }
    }

    /***********************\
    * Client -> Server Code *
    \***********************/

    /// receive keystrokes from the client
    /// send keystrokes down the key_tx
    async fn ws_c2s_task(
        mut ws_rx: SplitStream<WebSocket>,
        State(state): State<AppState>,
        session: Session,
    ) {
        let key_tx = state.key_tx.clone();
        while let Some(Ok(msg)) = ws_rx.next().await {
            if let ws::Message::Binary(body) = msg {
                if body.len() == 4 {
                    // interpret message
                    // 4 bytes long, little endian keystroke/char
                    let key_bytes: [u8; 4] = body[0..4].try_into().unwrap();
                    let key_int: u32 = u32::from_le_bytes(key_bytes);
                    if let Some(key) = char::from_u32(key_int) {
                        let session_id = session.id().expect(
                            "Session was not set before client broadcasted keystrokes to server",
                        );
                        let message_id: u32;
                        {
                            let session_to_message_ref = state.session_to_message.clone();
                            let session_to_message = (*session_to_message_ref).read().await;
                            message_id = *session_to_message.get(&session_id).expect("Client broadcasted keystrokes to server without having a message created.");
                        }

                        let keystroke = Keystroke { message_id, key };
                        if let Err(e) = key_tx.send(keystroke) {
                            eprintln!("Keystroke send error: {e}");
                        }
                        {
                            let messages_ref = state.messages.clone();
                            let mut messages = (*messages_ref).write().await;
                            match messages.get_mut(message_id as usize) {
                                Some(message) => message.text.push(key),
                                None => eprintln!(
                                    "Message id {message_id} not found in global messages vec"
                                ),
                            }
                        }
                    } else {
                        eprintln!("Bad key received: {:?}", body);
                    }
                } else {
                    eprintln!("Invalid character length: {:?}", body);
                }
            } else {
                eprintln!("Invalid ws key message received: {:?}", msg);
            }
        }
    }
}

async fn session_new_handler(state: State<AppState>, session: Session) -> impl IntoResponse {
    state
        .session_to_message
        .write()
        .await
        .remove(&session.id().unwrap());
    session.delete().await.ok();

    session.insert("preserve", true).await.ok();
    StatusCode::OK
}
