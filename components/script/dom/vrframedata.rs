/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use core::nonzero::NonZero;
use dom::bindings::codegen::Bindings::VRFrameDataBinding;
use dom::bindings::codegen::Bindings::VRFrameDataBinding::VRFrameDataMethods;
use dom::bindings::conversions::{slice_to_array_buffer_view, update_array_buffer_view};
use dom::bindings::error::Fallible;
use dom::bindings::js::Root;
use dom::bindings::num::Finite;
use dom::bindings::reflector::{Reflectable, Reflector, reflect_dom_object};
use dom::globalscope::GlobalScope;
use dom::vrpose::VRPose;
use js::jsapi::{Heap, JSContext, JSObject};
use std::cell::Cell;
use time;
use vr_traits::webvr;

#[dom_struct]
pub struct VRFrameData {
    reflector_: Reflector,
    left_proj: Heap<*mut JSObject>,
    left_view: Heap<*mut JSObject>,
    right_proj: Heap<*mut JSObject>,
    right_view: Heap<*mut JSObject>,
    pose: Root<VRPose>,
    timestamp: Cell<u64>
}

impl VRFrameData {

    #[allow(unrooted_must_root)]
    fn new_inherited(global: &GlobalScope) -> VRFrameData {

        let matrix = [1.0, 0.0, 0.0, 0.0,
                      0.0, 1.0, 0.0, 0.0,
                      0.0, 0.0, 1.0, 0.0,
                      0.0, 0.0, 0.0, 1.0f32];

        let mut framedata = VRFrameData {
            reflector_: Reflector::new(),
            left_proj: Heap::default(),
            left_view: Heap::default(),
            right_proj: Heap::default(),
            right_view: Heap::default(),
            pose: VRPose::new(&global, &Default::default()),
            timestamp: Cell::new(time::get_time().sec as u64)
        };

        framedata.left_proj.set(slice_to_array_buffer_view(global.get_cx(), &matrix));
        framedata.left_view.set(slice_to_array_buffer_view(global.get_cx(), &matrix));
        framedata.right_proj.set(slice_to_array_buffer_view(global.get_cx(), &matrix));
        framedata.right_view.set(slice_to_array_buffer_view(global.get_cx(), &matrix));

        framedata
    }

    pub fn new(global: &GlobalScope) -> Root<VRFrameData> {
        reflect_dom_object(box VRFrameData::new_inherited(global),
                           global,
                           VRFrameDataBinding::Wrap)
    }

    pub fn Constructor(global: &GlobalScope) -> Fallible<Root<VRFrameData>> {
        Ok(VRFrameData::new(global))
    }
}


impl VRFrameData {
    #[allow(unsafe_code)]
    pub fn update(&self, data: &webvr::VRFrameData) {
        unsafe {
            update_array_buffer_view(self.left_proj.get(), &data.left_projection_matrix);
            update_array_buffer_view(self.left_view.get(), &data.left_view_matrix);
            update_array_buffer_view(self.right_proj.get(), &data.right_projection_matrix);
            update_array_buffer_view(self.right_view.get(), &data.right_view_matrix);
        }
        self.pose.update(&self.global(), &data.pose);
        self.timestamp.set(data.timestamp);
    }
}

impl VRFrameDataMethods for VRFrameData {
    fn Timestamp(&self) -> Finite<f64> {
        Finite::wrap(self.timestamp.get() as f64)
    }

    #[allow(unsafe_code)]
    fn LeftProjectionMatrix(&self, _cx: *mut JSContext) -> NonZero<*mut JSObject> {
        unsafe { NonZero::new(self.left_proj.get()) }
    }

    #[allow(unsafe_code)]
    fn LeftViewMatrix(&self, _cx: *mut JSContext) -> NonZero<*mut JSObject> {
        unsafe { NonZero::new(self.left_view.get()) }
    }

    #[allow(unsafe_code)]
    fn RightProjectionMatrix(&self, _cx: *mut JSContext) -> NonZero<*mut JSObject> {
        unsafe { NonZero::new(self.right_proj.get()) }
    }

    #[allow(unsafe_code)]
    fn RightViewMatrix(&self, _cx: *mut JSContext) -> NonZero<*mut JSObject> {
        unsafe { NonZero::new(self.right_view.get()) }
    }

    fn Pose(&self) -> Root<VRPose> {
        Root::from_ref(&*self.pose)
    }
}