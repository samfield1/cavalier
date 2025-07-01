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
// TODO: examine binary_serde for sending Keystroke struct

#![allow(unused_imports)] // remove when development is further along
use js_sys::{ArrayBuffer, JsString, Uint8Array};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    ErrorEvent, MessageEvent, Request, RequestCredentials, RequestInit, RequestMode, Response,
    WebSocket, window,
};

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

#[wasm_bindgen(main)]
fn main() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    insert_message_div(6, "Feeder and Seeder");
    insert_message_div(5, "Sneed's Feed and Seed");

    spawn_local(async {
        if let Err(err) = run().await {
            console_log!("Error in main task: {:?}", err);
        }
    });
    Ok(())
}

#[wasm_bindgen]
pub async fn run() -> Result<(), JsValue> {
    let msgvec: Vec<Message> = get_msg().await?;
    for msg in &msgvec {
        insert_message_div(msg.id, &msg.text);
    }

    let ws_protocol = if window().unwrap().location().protocol()? == "https:" {
        "wss:"
    } else {
        "ws:"
    };
    let hostname = window().unwrap().location().hostname()?;
    let port = window().unwrap().location().port()?;

    let ws_events_url = format!(
        "{}//{}:{}{}",
        &ws_protocol, &hostname, &port, "/api/ws/events"
    );
    let ws_events = WebSocket::new(&ws_events_url)?; //.expect("Binding to events websocket failed");
    let ws_events_onmessage = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        match e.data().dyn_into::<JsString>() {
            Ok(event_json) => {
                let event_json_str = String::from(event_json);
                let event: Event = serde_json::from_str(&event_json_str[..])
                    .expect("Invalid event received from server");
                match event {
                    Event::MessageNew(message) => {
                        // Create new div for the message
                        insert_message_div(message.id, &message.text);
                    }
                    Event::MessageEnd => {}
                }
            }
            Err(e) => console_log!("Error receiving event: {:?}", e),
        }
    });
    ws_events.set_onmessage(Some(ws_events_onmessage.as_ref().unchecked_ref()));
    ws_events_onmessage.forget();

    let ws_key_url = format!("{}//{}:{}{}", &ws_protocol, &hostname, &port, "/api/ws/key");
    let ws_key = WebSocket::new(&ws_key_url)?;
    let ws_key_onmessage = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        match e.data().dyn_into::<ArrayBuffer>() {
            Ok(abuf) => {
                let array = Uint8Array::new(&abuf);
                //unpack keystroke and add it to the right message
            }
            Err(e) => console_log!("Error receiving keystroke: {:?}", e),
        }
    });
    ws_key.set_onmessage(Some(ws_key_onmessage.as_ref().unchecked_ref()));
    ws_key_onmessage.forget();

    let ws_key_send = ws_key.clone();
    let mut old_val = String::with_capacity(100);
    let on_keystroke = Closure::<dyn FnMut(_)>::new(move |event: web_sys::Event| {
        let new_val: String = String::from(event.target().unwrap().value_of().to_string());
        if new_val == old_val {
            return;
        }
        let key;
        if new_val.len() < old_val.len() {
            key = '\x08';
        } else {
            key = new_val.chars().last().unwrap_or_default();
        }
        let mut key_bytes: [u8; 4] = [0; 4];
        key.encode_utf8(&mut key_bytes);
        if let Err(err) = ws_key_send.send_with_u8_array(&key_bytes) {
            console_log!("Error sending key {}: {:?}", key, err);
        }
        old_val = new_val;
    });

    let document = window()
        .and_then(|win| win.document())
        .expect("Could not access the document");
    document
        .get_element_by_id("message-input")
        .expect("Message input does not exist")
        .add_event_listener_with_callback("input", on_keystroke.as_ref().unchecked_ref())?;
    on_keystroke.forget();

    Ok(())
}

fn insert_message_div(message_id: u32, text: &str) -> web_sys::Element {
    const MESSAGE_TMPL: &str = r#"<span class="message-leader"></span><span class="message-sender">&lt;Anon&gt;</span><span class="message-body" id="message-body-{id}">{body}</span>"#;

    let document = window()
        .and_then(|win| win.document())
        .expect("Could not access the document");

    let ui_messages_cont = document
        .get_element_by_id("messages-container")
        .expect("Message container does not exist");
    let ui_message_str = MESSAGE_TMPL
        .replace("{id}", &message_id.to_string())
        .replace("{body}", text);
    let ui_message_ele = document.create_element("div").unwrap();
    ui_message_ele.set_id(&message_id.to_string());
    ui_message_ele.set_class_name("message");
    ui_message_ele.set_inner_html(&ui_message_str);
    ui_messages_cont
        .append_child(&ui_message_ele)
        .expect("Unable to append msg to DOM");
    ui_message_ele
}

async fn get_msg() -> Result<Vec<Message>, JsValue> {
    // TODO: must handle request failed / server down. Currently results in JSON parse fail.
    let r_opts = RequestInit::new();
    r_opts.set_method("GET");
    r_opts.set_mode(RequestMode::SameOrigin);
    r_opts.set_credentials(RequestCredentials::Include);
    let msg_get_url = String::from("/api/msg/get");
    let r = Request::new_with_str_and_init(&msg_get_url, &r_opts)?;
    let window = window().unwrap();
    let resp_val = JsFuture::from(window.fetch_with_request(&r)).await?;
    let resp: Response = resp_val.dyn_into().unwrap();
    // let resp_json = JsFuture::from(resp.json()?).await?;
    // let resp_json_string = resp_json
    //     .as_string()
    //     .as_deref()
    //     .unwrap_or("JSON msgvec decode err")
    //     .to_string();
    let resp_json = JsFuture::from(resp.text()?).await?;
    let resp_json_str = resp_json.as_string();
    if resp_json_str.is_none() {
        return Err(JsValue::from_str("JSON to_string() returned None"));
    }
    serde_json::from_str::<Vec<Message>>(&resp_json_str.unwrap())
        .map_err(|err| JsValue::from_str(&err.to_string()))
}
