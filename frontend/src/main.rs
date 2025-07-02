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
// TODO: refactor entire application and remove wasteful, sinful, repeated lines
// This was a great learning experience though! And now that the basic functionality works, I have
// an excellent idea of how to organize this app properly. Things should be split by functionality
// (ws, json api, DOM methods, event listeners)
// and features (chat, user, auth, moderation, voting, etc).

// TODO: examine binary_serde for sending Keystroke struct

// TODO: Reevaluate message creation timing.
// Currently, a new message is created when the app loads and when Send is pressed. However, this
// causes unexpected ordering of messages, as a user may press Send, wait, and then begin typing.
// Instead, a new message should be created when the first keystroke of a new message is being
// created.
use js_sys::{ArrayBuffer, JsString, Uint8Array};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    BinaryType, Element, HtmlElement, HtmlInputElement, MessageEvent, Request, RequestCredentials,
    RequestInit, RequestMode, Response, WebSocket, window,
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

    new_session().await?;

    let current_message: Arc<Mutex<Option<Message>>> = Arc::new(Mutex::new(None)); //global current message cursor

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

    // Create onmessage function for the events websocket. Currently, this simply looks for
    // MessageNew events and adds a new message div to the DOM when one comes in.
    let ws_events = WebSocket::new(&ws_events_url)?; //.expect("Binding to events websocket failed");
    let ws_events_onmessage = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        match e.data().dyn_into::<JsString>() {
            Ok(event_json) => {
                let event_json_str = String::from(event_json);
                let event: Event = serde_json::from_str(&event_json_str[..])
                    .expect("Invalid event received from server");
                update_msg_visibility();
                match event {
                    Event::MessageNew(message) => {
                        // Create new div for the message
                        console_log!("Message id from server: {}", message.id);
                        insert_message_div(message.id, &message.text);
                        scroll_msg_cont_to_bottom();
                    }
                    Event::MessageEnd => {}
                }
            }
            Err(e) => console_log!("Error receiving event: {:?}", e),
        }
    });
    ws_events.set_onmessage(Some(ws_events_onmessage.as_ref().unchecked_ref()));
    ws_events_onmessage.forget();

    // Add onopen function which creates a new message when the websocket opens. This is the
    // initial message that the user starts with.
    let cur_msg_ref = current_message.clone();
    let ws_events_onopen = Closure::<dyn FnMut(_)>::new(move |_e: web_sys::Event| {
        let cur_msg_ref = cur_msg_ref.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let new_msg: Message = new_msg().await.unwrap();
            let mut cur_msg = cur_msg_ref.lock().expect("Couldn't set initial message");
            (*cur_msg) = Some(new_msg);
        })
    });
    ws_events.set_onopen(Some(ws_events_onopen.as_ref().unchecked_ref()));
    ws_events_onopen.forget();

    // Add onmessage function to websocket to listen for keystrokes and add them to the right
    // message.
    let ws_key_url = format!("{}//{}:{}{}", &ws_protocol, &hostname, &port, "/api/ws/key");
    let ws_key = WebSocket::new(&ws_key_url)?;
    ws_key.set_binary_type(BinaryType::Arraybuffer);
    let ws_key_onmessage = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        match e.data().dyn_into::<ArrayBuffer>() {
            Ok(abuf) => {
                let array = Uint8Array::new(&abuf);
                let bytes = array.to_vec();
                let key =
                    char::from_u32(u32::from_le_bytes(bytes[0..4].try_into().unwrap())).unwrap();
                let message_id = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
                update_message_div(message_id, key);
                update_msg_visibility();
            }
            Err(e) => console_log!("Error receiving keystroke: {:?}", e),
        }
    });
    ws_key.set_onmessage(Some(ws_key_onmessage.as_ref().unchecked_ref()));
    ws_key_onmessage.forget();

    // Add event listener to listen for keystrokes and broadcast them to the server
    let ws_key_send = ws_key.clone();
    let old_val: Arc<Mutex<String>> = Arc::new(Mutex::new(String::with_capacity(10)));
    let old_val_ref = old_val.clone();
    let on_keystroke = Closure::<dyn FnMut(_)>::new(move |event: web_sys::Event| {
        if let Ok(mut old_val) = old_val_ref.try_lock() {
            let input: HtmlInputElement = event
                .target()
                .unwrap()
                .dyn_into::<web_sys::HtmlInputElement>()
                .unwrap();
            let new_val: String = input.value();
            if new_val == *old_val {
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
            *old_val = new_val;
        } else {
            console_log!("on_keystroke: couldn't access input value tracker string. Try again.");
        }
    });

    let document = window()
        .and_then(|win| win.document())
        .expect("Could not access the document");
    document
        .get_element_by_id("message-input")
        .expect("Message input does not exist")
        .add_event_listener_with_callback("input", on_keystroke.as_ref().unchecked_ref())?;
    on_keystroke.forget();

    // Add event listener to reset the input value tracker and make a new message when Send is
    // clicked
    let sendbtn_current_message_ref = current_message.clone();
    let old_val_ref = old_val.clone();
    let on_sendbtn_click = Closure::<dyn FnMut(_)>::new(move |_event: web_sys::Event| {
        let sendbtn_current_message_ref = sendbtn_current_message_ref.clone();
        let old_val_ref = old_val_ref.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let new_msg: Message = new_msg().await.expect("Creating new message failed");
            let mut cur_msg = sendbtn_current_message_ref
                .lock()
                .expect("Couldn't set new message");
            (*cur_msg) = Some(new_msg);
            let input: HtmlInputElement = window()
                .unwrap()
                .document()
                .unwrap()
                .get_element_by_id("message-input")
                .unwrap()
                .dyn_into::<HtmlInputElement>()
                .unwrap();
            input.set_value("");
            old_val_ref
                .lock()
                .expect("sendbtn_click: couldn't lock input value tracker string.")
                .clear();
            input.focus().ok();
        })
    });
    document
        .get_element_by_id("send-button")
        .expect("Send button does not exist")
        .add_event_listener_with_callback("click", on_sendbtn_click.as_ref().unchecked_ref())?;
    on_sendbtn_click.forget();

    // Add event listener to prevent seeking in the text input, and to make the enter key trigger
    // Send's click function
    let on_input_keydown =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
            scroll_msg_cont_to_bottom();
            const DISABLED_KEYS: &[&str] = &["ArrowLeft", "ArrowRight"];
            if DISABLED_KEYS.contains(&event.key().as_str()) {
                event.prevent_default();
            }
            match event.key().as_str() {
                "Enter" => {
                    let submit_btn = window()
                        .unwrap()
                        .document()
                        .unwrap()
                        .get_element_by_id("send-button")
                        .expect("Submit button does not exist");

                    submit_btn.dyn_ref::<HtmlElement>().unwrap().click();
                }
                _ => {
                    console_log!("Key: {}", event.key());
                }
            }
        });
    document
        .get_element_by_id("message-input")
        .expect("Message input does not exist")
        .add_event_listener_with_callback("keydown", on_input_keydown.as_ref().unchecked_ref())?;
    on_input_keydown.forget();

    // maintain focus on input
    // TODO: revaluate. this prevents users from selecting and copying messages inside of the
    // chat message container.
    let on_input_blur = Closure::<dyn FnMut(_)>::new(move |_event: web_sys::Event| {
        window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("message-input")
            .expect("#message-input does not exist")
            .dyn_into::<HtmlInputElement>()
            .unwrap()
            .focus()
            .ok();
        window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                Closure::once_into_js(move || {
                    window()
                        .unwrap()
                        .document()
                        .unwrap()
                        .get_element_by_id("message-input")
                        .expect("#message-input does not exist")
                        .dyn_into::<HtmlInputElement>()
                        .unwrap()
                        .focus()
                        .ok();
                })
                .as_ref()
                .unchecked_ref(),
                0,
            )
            .ok();
    });
    document
        .get_element_by_id("message-input")
        .expect("Message input does not exist")
        .add_event_listener_with_callback("blur", on_input_blur.as_ref().unchecked_ref())?;
    on_input_blur.forget();

    // Set actual viewport height. This is for the CSS, and also so that phones don't spaz out when
    // the keyboard is opened.
    // TODO: make this less sinful. guh... look at that..
    let set_real_vh = Closure::<dyn FnMut(_)>::new(|_event: web_sys::Event| {
        let vh = window().unwrap().inner_height().unwrap().as_f64().unwrap();
        window()
            .unwrap()
            .document()
            .unwrap()
            .document_element()
            .unwrap()
            .dyn_into::<HtmlElement>()
            .unwrap()
            .style()
            .set_property("--real-vh", &format!("{}px", vh))
            .unwrap();
    });
    window()
        .unwrap()
        .add_event_listener_with_callback("resize", set_real_vh.as_ref().unchecked_ref())?;
    window().unwrap().add_event_listener_with_callback(
        "orientationchange",
        set_real_vh.as_ref().unchecked_ref(),
    )?;
    set_real_vh.forget();
    let vh = window().unwrap().inner_height().unwrap().as_f64().unwrap();
    window()
        .unwrap()
        .document()
        .unwrap()
        .document_element()
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap()
        .style()
        .set_property("--real-vh", &format!("{}px", vh))
        .unwrap();

    Ok(())
}

/// Update message div with a new char
fn update_message_div(message_id: u32, key: char) {
    let document = window()
        .and_then(|win| win.document())
        .expect("Could not access document");

    let ui_message_ele = document
        .get_element_by_id(&format!("message-body-{}", message_id.to_string()))
        .expect(&format!(
            "Could not get #message-body-{} to update it",
            message_id
        ));
    let mut text = ui_message_ele.inner_html();
    if key == '\x08' {
        text.pop().unwrap();
    } else {
        text.push(key);
    }
    ui_message_ele.set_inner_html(&text);
}

/// Add a new message div to the DOM.
fn insert_message_div(message_id: u32, text: &str) -> Element {
    const MESSAGE_TMPL: &str = r#"<div class="message-sender">&lt;Anon&gt;</div><div class="message-body" id="message-body-{id}">{body}</div>"#;

    let document = window()
        .and_then(|win| win.document())
        .expect("Could not access the document");

    let mut text_no_bksp = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\x08' {
            text_no_bksp.pop();
        } else {
            text_no_bksp.push(c);
        }
    }

    let ui_messages_cont = document
        .get_element_by_id("messages-container")
        .expect("Message container does not exist");
    let ui_message_str = MESSAGE_TMPL
        .replace("{id}", &message_id.to_string())
        .replace("{body}", &text_no_bksp);
    let ui_message_ele = document.create_element("div").unwrap();
    ui_message_ele.set_id(&format!("message-{}", &message_id.to_string()));
    ui_message_ele.set_class_name("message message-invisible");
    ui_message_ele.set_inner_html(&ui_message_str);
    ui_messages_cont
        .append_child(&ui_message_ele)
        .expect("Unable to append msg to DOM");
    ui_message_ele
}

/// hit the /msg/get endpoint
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

    let resp_json = JsFuture::from(resp.text()?).await?;
    let resp_json_str = resp_json.as_string();
    if resp_json_str.is_none() {
        return Err(JsValue::from_str("JSON to_string() returned None"));
    }
    serde_json::from_str::<Vec<Message>>(&resp_json_str.unwrap())
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

/// Hit the /msg/new endpoint
async fn new_msg() -> Result<Message, JsValue> {
    // TODO: must handle request failed / server down. Currently results in JSON parse fail.
    let r_opts = RequestInit::new();
    r_opts.set_method("GET");
    r_opts.set_mode(RequestMode::SameOrigin);
    r_opts.set_credentials(RequestCredentials::Include);
    let msg_get_url = String::from("/api/msg/new");
    let r = Request::new_with_str_and_init(&msg_get_url, &r_opts)?;
    let window = window().unwrap();
    let resp_val = JsFuture::from(window.fetch_with_request(&r)).await?;
    let resp: Response = resp_val.dyn_into().unwrap();

    let resp_json = JsFuture::from(resp.text()?).await?;
    let resp_json_str = resp_json.as_string();
    if resp_json_str.is_none() {
        return Err(JsValue::from_str("JSON to_string() returned None"));
    }
    serde_json::from_str::<Message>(&resp_json_str.unwrap())
        .map_err(|err| JsValue::from_str(&err.to_string()))
}

/// Reset the session variable when connecting to the server
async fn new_session() -> Result<(), JsValue> {
    // TODO: must handle request failed / server down. Currently results in JSON parse fail.
    let r_opts = RequestInit::new();
    r_opts.set_method("GET");
    r_opts.set_mode(RequestMode::SameOrigin);
    r_opts.set_credentials(RequestCredentials::Include);
    let msg_get_url = String::from("/api/session/new");
    let r = Request::new_with_str_and_init(&msg_get_url, &r_opts)?;
    let window = window().unwrap();
    let resp_val = JsFuture::from(window.fetch_with_request(&r)).await?;
    let resp: Response = resp_val.dyn_into().unwrap();
    let resp_text: String = JsFuture::from(resp.text()?).await?.as_string().unwrap();
    assert!(resp_text.is_empty());
    Ok(())
}

/// Scroll the message container to the bottom.
///
/// This makes the chat experience much less annoying.
fn scroll_msg_cont_to_bottom() {
    let msg_cont = window()
        .unwrap()
        .document()
        .unwrap()
        .get_element_by_id("messages-container")
        .expect("messages-container does not exist");
    msg_cont.set_scroll_top(msg_cont.scroll_height());
}

/// Sets messages with text == "" to display: none;
///
/// This runs on all messages
#[wasm_bindgen]
pub fn update_msg_visibility() {
    let msg_cont = window()
        .unwrap()
        .document()
        .unwrap()
        .get_element_by_id("messages-container")
        .expect("#messages-container does not exist");
    let msg_cont_children = msg_cont.children();
    let msgs: Vec<Element> = (0..msg_cont_children.length())
        .map(|i| {
            msg_cont_children
                .item(i)
                .unwrap()
                .dyn_into::<Element>()
                .unwrap()
        })
        .collect();
    for msg in msgs {
        let msg_body = msg
            .query_selector(".message-body")
            .expect("msg has no body!")
            .unwrap();
        let empty: bool = msg_body.inner_html().trim().is_empty();
        let classes = msg.class_list();
        if empty {
            classes.add_1("message-invisible").ok();
        } else {
            classes.remove_1("message-invisible").ok();
        }
        // msg.set_class_na
    }
}
