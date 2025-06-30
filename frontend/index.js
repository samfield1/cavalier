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


const ws_key = new WebSocket(`${uri_base}/api/ws/key`); 

let prev_val = "";
ws_key.onopen = () => {
    console.log('Keystroke WebSocket Connected');

    const input = document.getElementById("message-input");
    input.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            document.getElementById("submit-btn").click();
        }
    });

    input.addEventListener('input', (event) => {
        const new_val = event.target.value;
	if (new_val == prev_val) { return; }

	let key = '\0';
	if (new_val.length < prev_val.length) {
	    key = '\b';
	} else {
	    key = new_val[new_val.length - 1];
	}
	prev_val = new_val;
	ws_key.send(key);
    });
};

ws_key.onclose = (event) => {
    console.log("Keystroke WebSocket disconnected");
}

ws_key.onmessage = (event) => {
    console.log(`Keystroke received: ${event}`);

    const messageId = 0xFEEDFEEDn; //BigInt
    const keypress = '✅';
    const wsMsgBuffer = new ArrayBuffer(8);
    const bufferView = new DataView(wsMsgBuffer);
    const keypressCodepoint = keypress.codePointAt(0);
    bufferView.setUint32(0, keypressCodepoint);
    bufferView.setUint32(4, messageId);
    ws_key.send(wsMsgBuffer);


    const msg_id = 0;
    let msg = document.getElementById(`msg-${msg_id}`);
    if (!msg) {
        console.log(`Error: div for ${msg_id} does not exist. Has it been created?`);
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
    ws_key.send('\n');
    messageInput.value = '';
}

/********************************/
/* <!-- event handling code --> */
/********************************/

const ws_events = new WebSocket(`${uri_base}/api/ws/events`);
ws_events.onopen = (event) => {
    console.log("Events WebSocket Connected");
}
ws_events.onclose = (event) => {
    console.log("Events WebSocket Disconnected");
}
ws_events.onmessage = (event) => {
    console.log(event);

    //code to run when a new message event comes in
    // msg = document.createElement('div');   
    // const msg_cont = document.getElementById("msg-cont");
    // msg_cont.appendChild(msg);
    // msg.id = `msg-${msg_id}`;
}

/* vim: set sw=4 ts=4 sts=4 et: */
