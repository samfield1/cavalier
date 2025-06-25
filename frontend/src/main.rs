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

/// Just the "hello, world" so far
use web_sys::window;

fn main() {
    console_error_panic_hook::set_once();

    let document = window()
        .and_then(|win| win.document())
        .expect("Could not access the document");
    let body = document.body().expect("Could not access document.body");
    let text_node =
        document.create_text_node("This text is here because the Rust WASM is running properly.");
    body.append_child(text_node.as_ref())
        .expect("Failed to append text");
}
