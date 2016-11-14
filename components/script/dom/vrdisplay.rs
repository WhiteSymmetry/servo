/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use canvas_traits::CanvasMsg;
use core::ops::Deref;
use dom::bindings::callback::ExceptionHandling;
use dom::bindings::cell::DOMRefCell;
use dom::bindings::codegen::Bindings::PerformanceBinding::PerformanceBinding::PerformanceMethods;
use dom::bindings::codegen::Bindings::VRDisplayBinding;
use dom::bindings::codegen::Bindings::VRDisplayBinding::VRDisplayMethods;
use dom::bindings::codegen::Bindings::VRDisplayBinding::VREye;
use dom::bindings::codegen::Bindings::VRLayerBinding::VRLayer;
use dom::bindings::codegen::Bindings::WindowBinding::FrameRequestCallback;
use dom::bindings::codegen::Bindings::WindowBinding::WindowBinding::WindowMethods;
use dom::bindings::inheritance::Castable;
use dom::bindings::js::{JS, MutNullableHeap, MutHeap, Root};
use dom::bindings::num::Finite;
use dom::bindings::reflector::{Reflectable, reflect_dom_object};
use dom::bindings::refcounted::Trusted;
use dom::bindings::str::DOMString;
use dom::event::Event;
use dom::eventtarget::EventTarget;
use dom::globalscope::GlobalScope;
use dom::promise::Promise;
use dom::vrdisplaycapabilities::VRDisplayCapabilities;
use dom::vrdisplayevent::VRDisplayEvent;
use dom::vrstageparameters::VRStageParameters;
use dom::vreyeparameters::VREyeParameters;
use dom::vrframedata::VRFrameData;
use dom::vrpose::VRPose;
use dom::webglrenderingcontext::WebGLRenderingContext;
use js::jsapi::JSContext;
use ipc_channel::ipc;
use ipc_channel::ipc::IpcSender;
use util::thread::spawn_named;
use std::boxed::FnBox;
use std::cell::Cell;
use std::mem;
use std::rc::Rc;
use std::sync::mpsc;
use script_runtime::CommonScriptMsg;
use script_runtime::ScriptThreadEventCategory::WebVREvent;
use script_thread::Runnable;
use vr_traits::webvr;
use vr_traits::WebVRMsg;
use webrender_traits::VRCompositorCommand;

#[dom_struct]
pub struct VRDisplay {
    eventtarget: EventTarget,
    #[ignore_heap_size_of = "Defined in rust-webvr"]
    display: DOMRefCell<WebVRDisplayData>,
    depth_near: Cell<f64>,
    depth_far: Cell<f64>,
    presenting: Cell<bool>,
    left_eye_params: MutHeap<JS<VREyeParameters>>,
    right_eye_params: MutHeap<JS<VREyeParameters>>,
    capabilities: MutHeap<JS<VRDisplayCapabilities>>,
    stage_params: MutNullableHeap<JS<VRStageParameters>>,
    #[ignore_heap_size_of = "Defined in rust-webvr"]
    frame_data: DOMRefCell<WebVRFrameData>,
    #[ignore_heap_size_of = "Defined in rust-webvr"]
    layer: DOMRefCell<WebVRLayer>,
    layer_ctx: MutNullableHeap<JS<WebGLRenderingContext>>,
    #[ignore_heap_size_of = "Defined in rust-webvr"]
    compositor_id: DOMRefCell<Option<WebVRDeviceType>>,
    next_raf_id: Cell<u32>,
    /// List of request animation frame callbacks
    #[ignore_heap_size_of = "closures are hard"]
    raf_callback_list: DOMRefCell<Vec<(u32, Option<Box<FnBox(f64)>>)>>,
}

// Wrappers to include WebVR structs in a DOM struct
#[derive(Clone)]
pub struct WebVRDisplayData(webvr::VRDisplayData);
no_jsmanaged_fields!(WebVRDisplayData);

#[derive(Clone, Default)]
pub struct WebVRFrameData(webvr::VRFrameData);
no_jsmanaged_fields!(WebVRFrameData);

#[derive(Clone, Default)]
pub struct WebVRLayer(webvr::VRLayer);
no_jsmanaged_fields!(WebVRLayer);

#[derive(Clone)]
pub struct WebVRDeviceType(webvr::VRDeviceType);
no_jsmanaged_fields!(WebVRDeviceType);


impl VRDisplay {

    fn new_inherited(global: &GlobalScope, display:&webvr::VRDisplayData) -> VRDisplay {

        let stage = match display.stage_parameters {
            Some(ref params) => Some(VRStageParameters::new(&params, &global)),
            None => None
        };

        VRDisplay {
            eventtarget: EventTarget::new_inherited(),
            display: DOMRefCell::new(WebVRDisplayData(display.clone())),
            depth_near: Cell::new(0.01),
            depth_far: Cell::new(10000.0),
            presenting: Cell::new(false),
            left_eye_params: MutHeap::new(&*VREyeParameters::new(&display.left_eye_parameters, &global)),
            right_eye_params: MutHeap::new(&*VREyeParameters::new(&display.right_eye_parameters, &global)),
            capabilities: MutHeap::new(&*VRDisplayCapabilities::new(&display.capabilities, &global)),
            stage_params: MutNullableHeap::new(stage.as_ref().map(|v| v.deref())),
            frame_data: DOMRefCell::new(Default::default()),
            layer: DOMRefCell::new(Default::default()),
            layer_ctx: MutNullableHeap::default(),
            compositor_id: DOMRefCell::new(None),
            next_raf_id: Cell::new(1),
            raf_callback_list: DOMRefCell::new(vec![])
        }
    }

    pub fn new(global: &GlobalScope, display:&webvr::VRDisplayData) -> Root<VRDisplay> {
        reflect_dom_object(box VRDisplay::new_inherited(&global, &display),
                           global,
                           VRDisplayBinding::Wrap)
    }
}

impl Drop for VRDisplay {
    fn drop(&mut self) {

    }
}

impl VRDisplayMethods for VRDisplay {

    fn IsConnected(&self) -> bool {
        self.display.borrow().0.connected
    }

    fn IsPresenting(&self) -> bool {
        self.presenting.get()
    }

    fn Capabilities(&self) -> Root<VRDisplayCapabilities> {
        Root::from_ref(&*self.capabilities.get())
    }

    fn GetStageParameters(&self) -> Option<Root<VRStageParameters>> {
        self.stage_params.get().map(|s| Root::from_ref(&*s))
    }

    fn GetEyeParameters(&self, eye: VREye) -> Root<VREyeParameters> {
        match eye {
            VREye::Left => Root::from_ref(&*self.left_eye_params.get()),
            VREye::Right => Root::from_ref(&*self.right_eye_params.get())
        }
    }

    fn DisplayId(&self) -> u32 {
        self.display.borrow().0.display_id as u32
    }

    fn DisplayName(&self) -> DOMString {
        DOMString::from(self.display.borrow().0.display_name.clone())
    }

    fn GetFrameData(&self, frameData: &VRFrameData) -> bool {
        //TODO: sync with compositor
        if let Some(wevbr_sender) = self.webvr_thread() {
            let (sender, receiver) = ipc::channel().unwrap();
            wevbr_sender.send(WebVRMsg::GetFrameData(self.global().pipeline_id(),
                                                     self.get_display_id(),
                                                     self.depth_near.get(),
                                                     self.depth_far.get(),
                                                     sender)).unwrap();
            match receiver.recv().unwrap() {
                Ok(data) => {
                    self.frame_data.borrow_mut().0 = data;
                },
                Err(e) => {
                    error!("WebVR::GetFrameData: {:?}", e);
                }
            }
        }

        frameData.update(&self.frame_data.borrow().0);
        true
    }

    fn GetPose(&self) -> Root<VRPose> {
        VRPose::new(&self.global(), &self.frame_data.borrow().0.pose)
    }

    fn ResetPose(&self) -> () {
        if let Some(wevbr_sender) = self.webvr_thread() {
            wevbr_sender.send(WebVRMsg::ResetPose(self.global().pipeline_id(),
                                                  self.get_display_id(), 
                                                  None)).unwrap();
        }
    }

    fn DepthNear(&self) -> Finite<f64> {
        Finite::wrap(self.depth_near.get())
    }

    fn SetDepthNear(&self, value: Finite<f64>) -> () {
        self.depth_near.set(*value.deref());
    }

    fn DepthFar(&self) -> Finite<f64> {
        Finite::wrap(self.depth_far.get())
    }

    fn SetDepthFar(&self, value: Finite<f64>) -> () {
        self.depth_far.set(*value.deref());
    }

    fn RequestAnimationFrame(&self, callback: Rc<FrameRequestCallback>) -> u32 {
        if self.presenting.get() {
            let raf_id = self.next_raf_id.get();
            self.next_raf_id.set(raf_id + 1);
            let callback = move |now: f64| {
                let _ = callback.Call__(Finite::wrap(now), ExceptionHandling::Report);
            };
            self.raf_callback_list.borrow_mut().push((raf_id, Some(Box::new(callback))));
            raf_id 
        } else {
            self.global().as_window().RequestAnimationFrame(callback)
        }
    }

    fn CancelAnimationFrame(&self, handle: u32) -> () {
        if self.presenting.get() {
            let mut list = self.raf_callback_list.borrow_mut();
            if let Some(mut pair) = list.iter_mut().find(|pair| pair.0 == handle) {
                pair.1 = None;
            }
        } else {
            self.global().as_window().CancelAnimationFrame(handle);
        }
    }

    #[allow(unrooted_must_root)]
    fn RequestPresent(&self, layers: Vec<VRLayer>) -> Rc<Promise> {
        let promise = Promise::new(&self.global());
        // TODO: WebVR spec: this method must be called in response to a user gesture

        // WebVR spec: If canPresent is false the promise MUST be rejected
        if !self.display.borrow().0.capabilities.can_present {
            let msg = "VRDisplay canPresent is false".to_string();
            promise.reject_native(promise.global().get_cx(), &msg);
            return promise;
        }

        // Current WebVRSpec only allows 1 VRLayer if the VRDevice can present.
        // Future revisions of this spec may allow multiple layers to enable more complex rendering effects 
        // such as compositing WebGL and DOM elements together. 
        // That functionality is not allowed by this revision of the spec.
        if layers.len() != 1 {
            let msg = "The number of layers must be 1".to_string();
            promise.reject_native(promise.global().get_cx(), &msg);
            return promise;
        }

        // Parse and validate received VRLayer
        let layer = validate_layer(self.global().get_cx(), &layers[0]);

        if let Err(msg) = layer {
            let msg = msg.to_string();
            promise.reject_native(promise.global().get_cx(), &msg);
            return promise;
        }

        let (layer_bounds, layer_ctx) = layer.unwrap();

        // WebVR spec: Repeat calls while already presenting will update the VRLayers being displayed.
        if self.presenting.get() {
            *self.layer.borrow_mut() = layer_bounds;
            self.layer_ctx.set(Some(&layer_ctx));
            promise.resolve_native(promise.global().get_cx(), &());
            return promise;
        }
        
        // Request Present
        if let Some(wevbr_sender) = self.webvr_thread() {
            let (sender, receiver) = ipc::channel().unwrap();
            wevbr_sender.send(WebVRMsg::RequestPresent(self.global().pipeline_id(),
                                                       self.display.borrow().0.display_id,
                                                       sender))
                                                       .unwrap();
            match receiver.recv().unwrap() {
                Ok(compositor_id) => {
                    *self.layer.borrow_mut() = layer_bounds;
                    self.layer_ctx.set(Some(&layer_ctx));
                    self.init_present(compositor_id);
                    promise.resolve_native(promise.global().get_cx(), &());
                },
                Err(e) => {
                    promise.reject_native(promise.global().get_cx(), &e);
                }
            }
        } else {
            let msg = "Not available".to_string();
            promise.reject_native(promise.global().get_cx(), &msg);
        }

        promise
    }

    #[allow(unrooted_must_root)]
    fn ExitPresent(&self) -> Rc<Promise> {
        let promise = Promise::new(&self.global());

        // WebVR spec: If the VRDisplay is not presenting the promise MUST be rejected.
        if !self.presenting.get() {
            let msg = "VRDisplay is not presenting".to_string();
            promise.reject_native(promise.global().get_cx(), &msg);
            return promise;
        }

        // Exit present
        if let Some(wevbr_sender) = self.webvr_thread() {
            let (sender, receiver) = ipc::channel().unwrap();
            wevbr_sender.send(WebVRMsg::ExitPresent(self.global().pipeline_id(),
                                                    self.display.borrow().0.display_id,
                                                    sender))
                                                    .unwrap();
            match receiver.recv().unwrap() {
                Ok(()) => {
                    self.stop_present();
                    promise.resolve_native(promise.global().get_cx(), &());
                },
                Err(e) => {
                    promise.reject_native(promise.global().get_cx(), &e);
                }
            }
        } else {
            let msg = "Not available".to_string();
            promise.reject_native(promise.global().get_cx(), &msg);
        }

        promise
    }

    fn SubmitFrame(&self) -> () {
        if !self.presenting.get() {
            warn!("VRDisplay not presenting");
            return;
        }

        let api_sender = self.layer_ctx.get().unwrap().ipc_renderer();
        let compositor_id = self.compositor_id.borrow().as_ref().unwrap().0.as_u32();
        let layer = self.layer.borrow();
        let msg = VRCompositorCommand::SubmitFrame(compositor_id, layer.0.left_bounds, layer.0.right_bounds);
        api_sender.send(CanvasMsg::WebVR(msg)).unwrap();
    }
}

impl VRDisplay {

    fn webvr_thread(&self) -> Option<IpcSender<WebVRMsg>> {
        self.global().as_window().webvr_thread()
    }

    pub fn get_display_id(&self) -> u64 {
        self.display.borrow().0.display_id
    }

    pub fn update_display(&self, display: &webvr::VRDisplayData) {
        self.display.borrow_mut().0 = display.clone()
    }

    pub fn handle_webvr_event(&self, event: &webvr::VRDisplayEvent) {
        match *event {
            webvr::VRDisplayEvent::Connect(ref display) => {
                self.update_display(&display);
            },
            webvr::VRDisplayEvent::Disconnect(_id) => {
                self.display.borrow_mut().0.connected = false;
            },
            webvr::VRDisplayEvent::Activate(ref display, _) |
            webvr::VRDisplayEvent::Deactivate(ref display, _) |
            webvr::VRDisplayEvent::Blur(ref display) |
            webvr::VRDisplayEvent::Focus(ref display) => {
                self.update_display(&display);
                self.notify_event(&event);
            },
            webvr::VRDisplayEvent::PresentChange(ref display, presenting) => {
                self.update_display(&display);
                self.presenting.set(presenting);
                self.notify_event(&event);
            },
            webvr::VRDisplayEvent::Change(ref display) => {
                // Change event doesn't exist in WebVR spec.
                // So we update display data but don't notify JS.
                self.update_display(&display);
            }
        };
    }

    fn notify_event(&self, event: &webvr::VRDisplayEvent) {
        let root = Root::from_ref(&*self);
        let event = VRDisplayEvent::new_from_webvr(&self.global(), &root, &event);
        event.upcast::<Event>().fire(self.upcast());
    }

    fn init_present(&self, compositor_id: webvr::VRDeviceType) {
        self.presenting.set(true);
        *self.compositor_id.borrow_mut() = Some(WebVRDeviceType(compositor_id));
        let compositor_id = compositor_id.as_u32();
        let api_sender = self.layer_ctx.get().unwrap().ipc_renderer();
        let js_sender = self.global().script_chan();
        let address = Trusted::new(&*self);

        spawn_named("WebVR_RAF".into(), move || {
            let (sync_sender, sync_receiver) = ipc::channel().unwrap();
            let (raf_sender, raf_receiver) = mpsc::channel();
            // Initialize compositor
            api_sender.send(CanvasMsg::WebVR(VRCompositorCommand::Create(compositor_id))).unwrap();
            loop {
                // Run Sync Poses on Render thread
                let msg = VRCompositorCommand::SyncPoses(compositor_id, sync_sender.clone());
                api_sender.send(CanvasMsg::WebVR(msg)).unwrap();
                // Wait until Sync Poses ends
                if sync_receiver.recv().unwrap().is_err() {
                    // ExitPresent called or some error happened creating the compositor in webrender.
                    // Stop RAF thread.
                    return;
                }
                // Notify RAF callbacks
                let msg = box NotifyDisplayRAF {
                    address: address.clone(),
                    sender: raf_sender.clone()
                };
                js_sender.send(CommonScriptMsg::RunnableMsg(WebVREvent, msg)).unwrap();
                // Wait until RAF execution ends
                raf_receiver.recv().unwrap();
            }
        });
    }

    fn stop_present(&self) {
        self.presenting.set(false);
        let api_sender = self.layer_ctx.get().unwrap().ipc_renderer();
        let compositor_id = self.compositor_id.borrow().as_ref().unwrap().0.as_u32();
        let msg = VRCompositorCommand::Release(compositor_id);
        api_sender.send(CanvasMsg::WebVR(msg)).unwrap();
    }

    fn notify_raf(&self) {
        let mut callbacks = mem::replace(&mut *self.raf_callback_list.borrow_mut(), vec![]);
        let timing = self.global().as_window().Performance().Now();

        for (_, callback) in callbacks.drain(..) {
            if let Some(callback) = callback {
                callback(*timing);
            }
        }
    }
}

struct NotifyDisplayRAF {
    address: Trusted<VRDisplay>,
    sender: mpsc::Sender<()>
}

impl Runnable for NotifyDisplayRAF {
    fn name(&self) -> &'static str { "NotifyDisplayRAF" }

    fn handler(self: Box<Self>) {
        let display = self.address.root();
        display.notify_raf();
        self.sender.send(()).unwrap();
    }
}


// WebVR Spect: If the number of values in the leftBounds/rightBounds arrays
// is not 0 or 4 for any of the passed layers the promise is rejected
fn parse_bounds(src: &Option<Vec<Finite<f32>>>, dst: &mut [f32; 4]) -> Result<(), &'static str> {
    match *src {
        Some(ref values) => {
            if values.len() == 0 {
                return Ok(())
            } 
            if values.len() > 4 {
                return Err("The number of values in the leftBounds/rightBounds arrays must be 0 or 4")
            }
            for i in 0..4 {
                dst[i] = *values[i].deref();
            }
            Ok(())
        },
        None => Ok(())
    }
}

fn validate_layer(cx: *mut JSContext, layer: &VRLayer) -> Result<(WebVRLayer, Root<WebGLRenderingContext>), &'static str> {
    let ctx = layer.source.as_ref().map(|ref s| s.get_or_init_webgl_context(cx, None)).unwrap_or(None);
    if let Some(ctx) = ctx {
        let mut data = webvr::VRLayer::default();
        try!(parse_bounds(&layer.leftBounds, &mut data.left_bounds));
        try!(parse_bounds(&layer.rightBounds, &mut data.right_bounds));
        Ok((WebVRLayer(data), ctx))
    } else {
        Err("VRLayer source must be a WebGL Context")
    }
}