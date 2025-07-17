use anyhow::Result;
use prost::Message;

use crate::{Config, proto::options::Format};

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/valhalla.rs"));
}

#[allow(clippy::needless_lifetimes)] // clippy goes nuts with cxx
#[cxx::bridge]
mod ffi {
    /// Helper struct to provide an access to C++'s buffer with serialized response data.
    struct Response<'a> {
        /// Raw response data, either a JSON string or binary data.
        data: &'a [u8],
        /// [`options::Format`] format of the response.
        format: i32,
    }

    unsafe extern "C++" {
        include!("valhalla/src/actor.hpp");

        #[namespace = "boost::property_tree"]
        type ptree = crate::config::ffi::ptree;

        type Actor;
        fn new_actor(config: &ptree) -> Result<UniquePtr<Actor>>;
        fn cleanup(self: Pin<&mut Actor>);
        fn route(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;
        fn locate(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;
        fn matrix(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;
        fn optimized_route(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;
        fn isochrone(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;
        fn trace_route(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;
        fn trace_attributes(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;
        fn transit_available(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;
        fn expansion(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;
        fn centroid(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;
        fn status(self: Pin<&mut Actor>, request: &[u8]) -> Result<Response>;

        fn parse_api(json: &str, action: i32) -> Result<UniquePtr<CxxString>>;
    }
}

// Safety: `ffi::Actor` doesn't hold any reference to the shared state and all its methods require
// a mutable reference to `self`, so stronger borrowing rules apply here, preventing using mutable
// methods concurrently.
unsafe impl Send for ffi::Actor {}
unsafe impl Sync for ffi::Actor {}

/// Valhalla natively supports multiple response formats, such as JSON, OSRM-like JSON, PBF, and others.
/// This format is specified on per-request basis using [`proto::Options`] `format` field, selecting one of the
/// [`proto::options::Format`] options.
///
/// It is worth noting that at the moment not every endpoint supports all of the formats. For example,
/// [`Actor::locate()`] will always return a Valhalla JSON response regardless of the `format` field in the request.
#[derive(Debug, Clone)]
pub enum Response {
    /// Protocol Buffer response containing a structured [`proto::Api`] object.
    ///
    /// This format preserves full type information and is most efficient for
    /// programmatic access. Only available when [`proto::options::Format::Pbf`]
    /// is explicitly requested and the endpoint supports it.
    ///
    /// Set [`proto::PbfFieldSelector`] request option to get more detailed output.
    Pbf(Box<proto::Api>),
    /// JSON string response in either Valhalla or OSRM format.
    ///
    /// The JSON structure depends on which format was requested:
    /// - [`proto::options::Format::Json`] returns Valhalla-native JSON
    /// - [`proto::options::Format::Osrm`] returns OSRM-compatible JSON
    ///
    /// This is also the fallback format for most endpoints when other formats
    /// are not supported, like in the case of [`Actor::locate()`] that always
    /// returns a Valhalla JSON response.
    Json(String),
    /// Binary response for specialized formats.
    ///
    /// Currently used for:
    /// - GPX files (XML format for GPS data)
    /// - GeoTIFF files (raster geographic data)
    ///
    /// The actual format depends on the request and endpoint capabilities.
    Other(Vec<u8>),
}

impl From<ffi::Response<'_>> for Response {
    fn from(response: ffi::Response) -> Self {
        if response.format == Format::Pbf as i32 {
            let api = proto::Api::decode(response.data)
                .expect("Proper PBF data is guaranteed by Valhalla");
            Response::Pbf(Box::new(api))
        } else if response.format == Format::Json as i32 || response.format == Format::Osrm as i32 {
            Response::Json(String::from_utf8_lossy(response.data).into_owned())
        } else {
            Response::Other(response.data.to_owned())
        }
    }
}

pub struct Actor {
    inner: cxx::UniquePtr<ffi::Actor>,
    /// Buffer to reuse memory for encoded requests.
    buffer: Vec<u8>,
}

impl Actor {
    const INPUT_BUFFER_SIZE: usize = 1024; // 1 KiB is more than enough for most requests.

    /// ```rust
    /// let Ok(config) = valhalla::Config::from_file("path/to/config.json") else {
    ///     return; // Handle error appropriately
    /// };
    /// let actor = valhalla::Actor::new(&config);
    /// ```
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            inner: ffi::new_actor(config.inner())?,
            buffer: Vec::with_capacity(Self::INPUT_BUFFER_SIZE),
        })
    }

    pub fn route(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::route, request)
    }

    pub fn locate(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::locate, request)
    }

    pub fn matrix(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::matrix, request)
    }

    pub fn optimized_route(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::optimized_route, request)
    }

    pub fn isochrone(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::isochrone, request)
    }

    pub fn trace_route(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::trace_route, request)
    }

    pub fn trace_attributes(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::trace_attributes, request)
    }

    pub fn transit_available(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::transit_available, request)
    }

    pub fn expansion(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::expansion, request)
    }

    pub fn centroid(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::centroid, request)
    }

    pub fn status(&mut self, request: &proto::Api) -> Result<Response> {
        self.act(ffi::Actor::status, request)
    }

    /// Generic helper function to process request encoding, calling the endpoint and handling cleanup.
    fn act<F>(&mut self, endpoint: F, request: &proto::Api) -> Result<Response>
    where
        F: for<'a> FnOnce(
            std::pin::Pin<&'a mut ffi::Actor>,
            &'a [u8],
        ) -> Result<ffi::Response<'a>, cxx::Exception>,
    {
        self.buffer.clear();
        self.buffer.reserve(request.encoded_len());
        request.encode_raw(&mut self.buffer);
        println!("Request size is {}", self.buffer.len());

        let result = match endpoint(self.inner.as_mut().unwrap(), &self.buffer) {
            Ok(response) => Ok(Response::from(response)),
            Err(err) => Err(anyhow::Error::from(err)),
        };

        // `ffi::Response::data` holds a slice to the internal buffer from which we create `Response`.
        // Once we are done with the response, we can safely call cleanup to release any internal resources.
        self.inner.as_mut().unwrap().cleanup();

        // Single huge request can lead to excessive memory usage, let's keep it manageable.
        if self.buffer.capacity() > Self::INPUT_BUFFER_SIZE {
            self.buffer = Vec::with_capacity(Self::INPUT_BUFFER_SIZE);
        }

        result
    }

    /// Helper function to convert a Valhalla JSON string to a Valhalla API request.
    pub fn parse_api(json: &str, action: proto::options::Action) -> Result<proto::Api> {
        if json.is_empty() {
            // Empty string is a special case downstream, so we can return an error here.
            return Err(anyhow::anyhow!("Failed to parse json request"));
        }

        let cxx_string = ffi::parse_api(json, action as i32)?;
        Ok(proto::Api::decode(cxx_string.as_bytes())?)
    }
}
