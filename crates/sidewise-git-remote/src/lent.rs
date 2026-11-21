//! Lends one `Http` backend to several gix transports: the fetch needs a fresh transport per
//! connection, but owns only one backend, whose byte budget must span both connections.

use gix_transport::Protocol;
use gix_transport::client::blocking_io::Transport;
use gix_transport::client::blocking_io::http::{
    GetResponse, Http, PostBodyDataKind, PostResponse, connect_http,
};

/// A fresh protocol v2 transport for one connection, borrowing `http`. gix's HTTP transport keeps
/// the first response's reader, so a second handshake needs its own transport.
pub(crate) fn lend<'a, C: Http>(http: &'a mut C, url: &gix::Url) -> impl Transport + 'a {
    connect_http(Lent(http), url.clone(), Protocol::V2, false)
}

/// Lends one `Http` backend to a transport; every call goes to the borrowed backend.
struct Lent<'a, C>(&'a mut C);

impl<C: Http> Http for Lent<'_, C> {
    type Headers = C::Headers;
    type ResponseBody = C::ResponseBody;
    type PostBody = C::PostBody;

    fn get(
        &mut self,
        url: &str,
        base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> gix::Result<GetResponse<C::Headers, C::ResponseBody>> {
        self.0.get(url, base_url, headers)
    }

    fn post(
        &mut self,
        url: &str,
        base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
        body: PostBodyDataKind,
    ) -> gix::Result<PostResponse<C::Headers, C::ResponseBody, C::PostBody>> {
        self.0.post(url, base_url, headers, body)
    }

    fn configure(&mut self, config: &dyn std::any::Any) -> gix::Result {
        self.0.configure(config)
    }

    fn redirected_base_url(&self) -> Option<String> {
        self.0.redirected_base_url()
    }
}
