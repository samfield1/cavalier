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

#![allow(unused_imports)] // remove when development is further along
use js_sys::{ArrayBuffer, JsString, Uint8Array};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::{ErrorEvent, MessageEvent, WebSocket, window};

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

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

fn main() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let document = window()
        .and_then(|win| win.document())
        .expect("Could not access the document");
    let body = document.body().expect("Could not access document.body");
    let text_node =
        document.create_text_node("This text is here because the Rust WASM is running properly.");
    body.append_child(text_node.as_ref())
        .expect("Failed to append text");

    #[cfg(debug_assertions)]
    const BASE_URL: &'static str = "ws://localhost:3000/api{}";
    #[cfg(not(debug_assertions))]
    const BASE_URL: &'static str = "wss://cavalier.samfield.net/api{}";

    let base_url = String::from(BASE_URL);
    let ws_events_url = &(base_url + "/ws/events")[..];
    let ws_events = WebSocket::new(ws_events_url)?; //.expect("Binding to events websocket failed");
    ws_events.set_binary_type(web_sys::BinaryType::Arraybuffer);

    let ws_events_onmessage = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        match e.data().dyn_into::<JsString>() {
            Ok(event_json) => {}
            Err(e) => console_log!("Error receiving event: {:?}", e),
        }
    });
    ws_events.set_onmessage(Some(ws_events_onmessage.as_ref().unchecked_ref()));
    ws_events_onmessage.forget();

    let ws_key_onmessage = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        match e.data().dyn_into::<ArrayBuffer>() {
            Ok(abuf) => {
                let array = Uint8Array::new(&abuf);
            }
            Err(e) => console_log!("Error receiving event: {:?}", e),
        }
    });

    // ws_events.set_onmessage(|ws_message| {
    //     println!("New event ws message: {:?}", ws_message);
    // });
    Ok(())
}
