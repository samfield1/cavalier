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


/*************************************/
 /* <!-- keystroke handler --> */ 
/*************************************/
const hostname = window.location.hostname;
const isLocalhost = hostname.includes("localhost") || hostname === '127.0.0.1';
const protocol = (isLocalhost) ? "ws" : "wss";
const port = (isLocalhost) ? "3000" : "80";
const uri_base = `${protocol}://${hostname}:${port}`
const ws_key = new WebSocket(`${uri_base}/api/ws-key`);

let prev_val = "";
ws_key.onopen = () => {
    console.log('Client -> Server WebSocket Connected');

    const input = document.getElementById("message-input");
    input.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            document.getElementById("submit-btn").click();
        } else {
            // an alterate way to get the keypresses to the server, except it only shows "Process" for regular keys.
            // keyCode is another thing to look at
            /* console.log(event.key);
            ws_c2s.send(JSON.stringify({ "key": event.key })); */
        }
    });

    input.addEventListener('input', (event) => {
        const new_val = event.target.value;
        const messageId = 0xFEEDFEEDFEEDFEEDn; //BigInt
        const keypress = '✅';
        const wsMsgBuffer = new ArrayBuffer(12);
        const bufferView = new DataView(wsMsgBuffer);
        bufferView.setBigInt64(0, messageId);
        const keypressCodepoint = keypress.codePointAt(0);
        bufferView.setUint32(8, keypressCodepoint);
        ws_key.send(wsMsgBuffer);
        /*if (new_val != prev_val) {
            let key = 0;
            if (new_val.length < prev_val.length) {
                key = '\b';
            } else {
                key = new_val[new_val.length -1];
            }*/
        
            //ws_c2s.send(key);
            //prev_val = new_val;
        //}
    });
};

ws_key.onclose = (event) => {
    console.log("Events WebSocket Connected");
}

ws_key.onmessage = (event) => {
    const msg_id = 0;
    let msg = document.getElementById(`msg-${msg_id}`);
    if (!msg) {
        msg = document.createElement('div');   
        const msg_cont = document.getElementById("msg-cont");
        msg_cont.appendChild(msg);
        msg.id = `msg-${msg_id}`;
   }
    let text = msg.innerHTML;
    const key = event.data;
    if (key === '\b') {
        text = text.slice(0, -1);
    } else {
        text += key;
    }

    msg.innerHTML = text;
};

let sendMessage = () => {
    ws_c2s.send('\n');
    messageInput.value = '';
}

/********************************/
/* <!-- event handling code --> */
/********************************/

const ws_events = new WebSocket(`${uri_base}/api/ws-events`);
ws_events.onopen = (event) => {
    console.log("Events WebSocket Disconnected");
}
ws_events.onclose = (event) => {
    console.log("Events WebSocket Connected");
}
ws_events.onmessage = (event) => {
    /* Receive events. Send events. Global state */
}
