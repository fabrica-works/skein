//! Window fetch carries exchanges; the pure half bounds and retires answers.
use super::{Exchange, Report};
use crate::inbox::Inbox;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{
    AbortController, ReadableStreamDefaultReader, Request, RequestCredentials, RequestInit, RequestMode, Response,
};

pub struct Fetch<R> {
    shared: Rc<Shared<R>>,
}
struct Shared<R> {
    fetch: RefCell<super::Fetch>,
    controllers: RefCell<BTreeMap<u64, AbortController>>,
    inbox: Rc<Inbox<R>>,
    wrap: fn(Report) -> R,
    stream: RefCell<Option<WebStream>>,
}
struct WebStream {
    ticket: u64,
    controller: AbortController,
    reader: Option<ReadableStreamDefaultReader>,
    reading: bool,
}
impl<R> Clone for Fetch<R> {
    fn clone(&self) -> Self {
        Self { shared: Rc::clone(&self.shared) }
    }
}
impl<R: 'static> Fetch<R> {
    #[must_use]
    pub fn new(capacity: u32, inbox: Rc<Inbox<R>>, wrap: fn(Report) -> R) -> Self {
        Self {
            shared: Rc::new(Shared {
                fetch: RefCell::new(super::Fetch::new(capacity)),
                controllers: RefCell::new(BTreeMap::new()),
                inbox,
                wrap,
                stream: RefCell::new(None),
            }),
        }
    }
    pub fn exchange(&self, exchange: &Exchange) {
        let ticket = self.shared.fetch.borrow_mut().begin(exchange.id, exchange.bound);
        let Some(ticket) = ticket else {
            self.shared.inbox.raise((self.shared.wrap)(Report::Failed { exchange: exchange.id }));
            return;
        };
        let Ok(controller) = AbortController::new() else {
            self.shared.fail(ticket);
            return;
        };
        self.shared.controllers.borrow_mut().insert(ticket, controller.clone());
        let Ok(request) = request(exchange, &controller) else {
            self.shared.fail(ticket);
            return;
        };
        let promise = web_sys::window().expect("browser window").fetch_with_request(&request);
        let shared = Rc::clone(&self.shared);
        let failed = Rc::clone(&shared);
        listen(
            &promise,
            move |value| {
                let Ok(response) = value.dyn_into::<Response>() else {
                    shared.fail(ticket);
                    return;
                };
                let status = response.status();
                if let Some(body) = response.body() {
                    let Ok(reader) = body.get_reader().dyn_into::<ReadableStreamDefaultReader>() else {
                        shared.fail(ticket);
                        return;
                    };
                    read(Rc::clone(&shared), ticket, status, reader.clone());
                } else {
                    shared.finish(ticket, status);
                }
            },
            move |_| failed.fail(ticket),
        );
    }
    pub fn open_stream(&self, path: &[u8], headers: &[crate::transport::Header], bound: u32) {
        self.close_stream();
        let ticket = self.shared.fetch.borrow_mut().open_stream(bound);
        let Ok(controller) = AbortController::new() else {
            self.shared.end_stream(ticket);
            return;
        };
        self.shared.stream.replace(Some(WebStream {
            ticket,
            controller: controller.clone(),
            reader: None,
            reading: false,
        }));
        let headers = headers
            .iter()
            .map(|(name, value)| Ok((std::str::from_utf8(name)?.into(), std::str::from_utf8(value)?.into())))
            .collect::<Result<Vec<_>, std::str::Utf8Error>>();
        let Ok(headers) = headers else {
            self.shared.end_stream(ticket);
            return;
        };
        let exchange = Exchange { id: 0, path: path.into(), method: "GET".into(), headers, body: Box::from([]), bound };
        let Ok(request) = request(&exchange, &controller) else {
            self.shared.end_stream(ticket);
            return;
        };
        let promise = web_sys::window().expect("browser window").fetch_with_request(&request);
        let (shared, failed) = (Rc::clone(&self.shared), Rc::clone(&self.shared));
        listen(
            &promise,
            move |value| {
                let Ok(response) = value.dyn_into::<Response>() else {
                    shared.end_stream(ticket);
                    return;
                };
                if response.status() != 200 {
                    shared.refuse_stream(ticket, response.status());
                    return;
                }
                let reader =
                    response.body().and_then(|body| body.get_reader().dyn_into::<ReadableStreamDefaultReader>().ok());
                let Some(reader) = reader else {
                    shared.end_stream(ticket);
                    return;
                };
                let opened = {
                    let mut stream = shared.stream.borrow_mut();
                    if let Some(stream) = stream.as_mut().filter(|stream| stream.ticket == ticket) {
                        stream.reader = Some(reader);
                        true
                    } else {
                        false
                    }
                };
                if opened {
                    shared.inbox.raise((shared.wrap)(Report::StreamOpened));
                }
            },
            move |_| failed.end_stream(ticket),
        );
    }
    pub fn close_stream(&self) {
        let ticket = self.shared.fetch.borrow().stream_ticket();
        if let Some(ticket) = ticket {
            self.shared.end_stream(ticket);
        }
    }
    pub fn read_on(&self) {
        read_stream(&self.shared);
    }
    pub fn abandon(&self, id: u32) {
        let ticket = self.shared.fetch.borrow_mut().abandon(id);
        if let Some(ticket) = ticket
            && let Some(controller) = self.shared.controllers.borrow_mut().remove(&ticket)
        {
            controller.abort();
        }
    }
}
impl<R: 'static> Shared<R> {
    fn end_stream(&self, ticket: u64) {
        let report = self.fetch.borrow_mut().end_stream(ticket);
        self.ended_stream(report);
    }
    fn refuse_stream(&self, ticket: u64, status: u16) {
        let report = self.fetch.borrow_mut().refuse_stream(ticket, status);
        self.ended_stream(report);
    }
    fn ended_stream(&self, report: Option<Report>) {
        if let Some(report) = report {
            let stream = self.stream.borrow_mut().take();
            if let Some(stream) = stream {
                stream.controller.abort();
            }
            self.inbox.raise((self.wrap)(report));
        }
    }
    fn report(&self, ticket: u64, report: Option<Report>) {
        if let Some(report) = report {
            self.controllers.borrow_mut().remove(&ticket);
            self.inbox.raise((self.wrap)(report));
        }
    }
    fn fail(&self, ticket: u64) {
        if let Some(controller) = self.controllers.borrow_mut().remove(&ticket) {
            controller.abort();
        }
        let report = self.fetch.borrow_mut().fail(ticket);
        self.report(ticket, report);
    }
    fn finish(&self, ticket: u64, status: u16) {
        let report = self.fetch.borrow_mut().finish(ticket, status);
        self.report(ticket, report);
    }
}
fn request(exchange: &Exchange, controller: &AbortController) -> Result<Request, JsValue> {
    let path = std::str::from_utf8(&exchange.path).map_err(|_| JsValue::from_str("path is UTF-8"))?;
    if !path.starts_with('/') || path.starts_with("//") || path.contains('\\') {
        return Err(JsValue::from_str("exchange path stays on the origin"));
    }
    let init = RequestInit::new();
    init.set_method(&exchange.method);
    init.set_credentials(RequestCredentials::SameOrigin);
    init.set_mode(RequestMode::SameOrigin);
    init.set_signal(Some(&controller.signal()));
    let headers = web_sys::Headers::new()?;
    for (name, value) in &exchange.headers {
        headers.append(name, value)?;
    }
    init.set_headers_headers(&headers);
    if !exchange.body.is_empty() {
        init.set_body_opt_u8_array(Some(&js_sys::Uint8Array::from(exchange.body.as_ref())));
    }
    Request::new_with_str_and_init(path, &init)
}
fn read<R: 'static>(shared: Rc<Shared<R>>, ticket: u64, status: u16, reader: ReadableStreamDefaultReader) {
    let failed = Rc::clone(&shared);
    listen(
        &reader.read(),
        move |result| {
            if shared.fetch.borrow().remaining(ticket).is_none() {
                return;
            }
            let done = js_sys::Reflect::get(&result, &JsValue::from_str("done")).ok().and_then(|v| v.as_bool());
            if done == Some(true) {
                shared.finish(ticket, status);
                return;
            }
            let Ok(value) = js_sys::Reflect::get(&result, &JsValue::from_str("value")) else {
                shared.fail(ticket);
                return;
            };
            let Ok(bytes) = value.dyn_into::<js_sys::Uint8Array>() else {
                shared.fail(ticket);
                return;
            };
            let remaining = shared.fetch.borrow().remaining(ticket).expect("exchange still live");
            if bytes.length() > remaining {
                shared.fail(ticket);
                return;
            }
            let report = shared.fetch.borrow_mut().piece(ticket, &bytes.to_vec());
            shared.report(ticket, report);
            read(Rc::clone(&shared), ticket, status, reader.clone());
        },
        move |_| failed.fail(ticket),
    );
}
fn listen(promise: &js_sys::Promise, resolve: impl FnMut(JsValue) + 'static, reject: impl FnMut(JsValue) + 'static) {
    let resolve = Closure::wrap(Box::new(resolve) as Box<dyn FnMut(JsValue)>);
    let reject = Closure::wrap(Box::new(reject) as Box<dyn FnMut(JsValue)>);
    let _next = promise.then2(&resolve, &reject);
    // The promise holds the functions; browser GC frees both captures after settlement.
    let _resolve = resolve.into_js_value();
    let _reject = reject.into_js_value();
}

fn read_stream<R: 'static>(shared: &Rc<Shared<R>>) {
    let report = shared.fetch.borrow_mut().read_on();
    if let Some(report) = report {
        shared.inbox.raise((shared.wrap)(report));
        return;
    }
    let next = {
        let mut slot = shared.stream.borrow_mut();
        let Some(stream) = slot.as_mut() else {
            return;
        };
        if stream.reading {
            return;
        }
        let Some(reader) = stream.reader.clone() else {
            return;
        };
        stream.reading = true;
        (stream.ticket, reader)
    };
    let (ticket, reader) = next;
    let (shared, failed) = (Rc::clone(shared), Rc::clone(shared));
    listen(
        &reader.read(),
        move |result| {
            if shared.fetch.borrow().stream_ticket() != Some(ticket) {
                return;
            }
            if js_sys::Reflect::get(&result, &JsValue::from_str("done")).ok().and_then(|v| v.as_bool()) == Some(true) {
                shared.end_stream(ticket);
                return;
            }
            let bytes = js_sys::Reflect::get(&result, &JsValue::from_str("value"))
                .ok()
                .and_then(|value| value.dyn_into::<js_sys::Uint8Array>().ok());
            let Some(bytes) = bytes else {
                shared.end_stream(ticket);
                return;
            };
            if let Some(stream) = shared.stream.borrow_mut().as_mut() {
                stream.reading = false;
            }
            let report = shared.fetch.borrow_mut().stream_piece(ticket, bytes.to_vec().into());
            if let Some(report) = report {
                shared.inbox.raise((shared.wrap)(report));
            } else {
                read_stream(&shared);
            }
        },
        move |_| failed.end_stream(ticket),
    );
}
