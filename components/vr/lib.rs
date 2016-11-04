/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![feature(plugin)]
#![feature(custom_derive)]
#![feature(proc_macro)]
#![deny(unsafe_code)]

extern crate ipc_channel;
extern crate msg;
extern crate util;
extern crate script_traits;
extern crate vr_traits;

mod webvr_thread;
pub use webvr_thread::WebVRThread;