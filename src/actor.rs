#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("valhalla/tyr/actor.h");
        include!("valhalla/src/actor.hpp");

        type Actor;
        fn new_actor() -> UniquePtr<Actor>;
        fn trace_route(self: Pin<&mut Actor>, request: &[u8]) -> Result<String>;
        fn race_attributes(self: Pin<&mut Actor>, request: &[u8]) -> Result<String>;
    }
}

pub struct Actor(cxx::UniquePtr<ffi::Actor>);

impl Actor {
    pub fn new() -> Self {
        Self(ffi::new_actor())
    }

    pub fn trace_route(&mut self, request: &[u8]) -> String {
        self.0.as_mut().unwrap().trace_route(request).unwrap()
    }
    pub fn race_attributes(&mut self, request: &[u8]) -> String {
        self.0.as_mut().unwrap().race_attributes(request).unwrap()
    }
}
