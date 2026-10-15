//! The reference shell: start the parts, carry reports, apply the view's patches.
#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]
use skein_lib::Queue;
use skein_ui::{Act, NodeId, Owner, View, look::Names};
use skein_ui_example_view::{Duties, Report as ViewReport, State, limits, places};
use skein_ui_web::{
    Look, fetch,
    inbox::Inbox,
    page::{Page, web::Dom},
    platform,
    store::Store,
    transport::{self, Carried, Requests},
    wasm_bindgen,
};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, closure::Closure};

const NAMES: &[Names<'_>] = &[Names { owner: Owner { number: 1 }, prefix: b"notes", styles: &[], regions: &[] }];
// Each part wraps its happening in the application's report type.
enum Report {
    Start,
    Due,
    Act(NodeId, Act),
    Fetch(fetch::Report),
    Carried(Carried),
}
struct Shell {
    state: State,
    view: View,
    page: Rc<RefCell<Page<'static>>>,
    dom: Dom,
    platform: platform::Browser<Report>,
    store: Store,
    timer: Option<i32>,
    timer_callback: Closure<dyn FnMut()>,
}

#[wasm_bindgen(start)]
pub fn start() {
    let window = web_sys::window().expect("browser window");
    let document = window.document().expect("browser document");
    let mount = document.get_element_by_id("app").expect("the bundle supplies #app");
    // Read the person's facts once. Later turns read only the two clocks.
    let facts = platform::facts();
    let location = window.location();
    let path = location.pathname().expect("page path") + &location.search().expect("page query");
    let inbox = Inbox::new();
    let fetch = fetch::web::Fetch::new(9, Rc::clone(&inbox), Report::Fetch);
    let platform = platform::Browser::new(fetch);
    let store = Store::new(b"notes-", None);
    let records: Vec<_> = store
        .read_all(&platform)
        .into_iter()
        .map(|record| skein_ui_example_view::Record { key: record.key, value: record.value })
        .collect();
    let state = State::start(facts.seed, path.as_bytes(), &records, facts.offset, &facts.languages);
    let limits = limits::limits();
    let page = Rc::new(RefCell::new(Page::new(
        Look { names: NAMES, pictures: &[], draw: no_drawing },
        &limits,
        b"notes",
        facts.primary,
    )));
    let dom = Dom::new(mount, &page, &inbox, Report::Act);
    let reports = Rc::clone(&inbox);
    let timer_callback = Closure::new(move || reports.raise(Report::Due));
    let mut shell = Shell { state, view: View::new(&limits), page, dom, platform, store, timer: None, timer_callback };
    // The inbox keeps the shell and its listeners alive and forbids a turn inside a turn.
    inbox.start(Box::new(move |inbox| shell.turn(inbox)));
    inbox.raise(Report::Start);
}
impl Shell {
    fn turn(&mut self, inbox: &Inbox<Report>) {
        let (now, wall) = platform::clocks();
        while let Some(report) = inbox.next() {
            let mut read_on = false;
            let report = match report {
                Report::Start | Report::Due => Some(ViewReport::Due),
                Report::Act(node, act) => self.view.act(node, act).map(ViewReport::Act),
                Report::Fetch(fetch::Report::Answered { exchange, status, body }) => {
                    Some(ViewReport::Answered { exchange, status, body })
                }
                Report::Fetch(fetch::Report::Failed { exchange }) | Report::Carried(Carried::Failed { exchange }) => {
                    Some(ViewReport::Failed { exchange })
                }
                Report::Fetch(fetch::Report::StreamOpened) => {
                    read_on = true;
                    Some(ViewReport::StreamOpened)
                }
                Report::Fetch(fetch::Report::StreamPiece { bytes }) => {
                    read_on = true;
                    Some(ViewReport::StreamPiece { bytes })
                }
                Report::Fetch(fetch::Report::StreamEnded) => Some(ViewReport::StreamEnded),
                Report::Carried(Carried::Written { .. }) => None,
                Report::Carried(Carried::Refused { .. }) => Some(ViewReport::Failed { exchange: 0 }),
            };
            let Some(report) = report else { continue };
            let mut duties = Duties {
                writes: Queue::with_capacity(8),
                removes: Queue::with_capacity(8),
                exchanges: Queue::with_capacity(8),
                show: None,
                stream: None,
            };
            self.state.step(report, now, wall, &mut duties);
            let mut requests = Requests::default();
            while let Some(record) = duties.writes.pop() {
                requests.writes.push(skein_ui_web::store::Record { key: record.key, value: record.value });
            }
            while let Some(key) = duties.removes.pop() {
                requests.removes.push(key);
            }
            while let Some(exchange) = duties.exchanges.pop() {
                requests.exchanges.push(fetch::Exchange {
                    id: exchange.id,
                    path: exchange.path,
                    method: if exchange.post { "POST" } else { "GET" }.into(),
                    headers: vec![("Content-Type".into(), "application/octet-stream".into())],
                    body: exchange.body,
                    bound: exchange.bound,
                });
            }
            requests.stream = duties.stream.map(|duty| match duty {
                skein_ui_example_view::StreamDuty::Open { path, headers, bound } => {
                    transport::StreamDuty::Open { path, headers: headers.into_vec(), bound }
                }
                skein_ui_example_view::StreamDuty::Close => transport::StreamDuty::Close,
            });
            let mut carried = Vec::new();
            transport::carry(requests, &mut self.store, &mut self.platform, &mut carried);
            for report in carried {
                inbox.raise(Report::Carried(report));
            }
            if read_on {
                self.platform.fetch.read_on();
            }
        }
        // Build once after all waiting reports; document work can raise the next turn's reports.
        let mut patches = Queue::with_capacity(limits::limits().patches);
        self.state.build(&mut self.view, &mut patches);
        let patches: Vec<_> = std::iter::from_fn(|| patches.pop()).collect();
        let mut ops = Vec::new();
        self.page.borrow_mut().apply(&patches, &mut ops);
        // Give each link the codec's path, so the browser can follow it without acting.
        let links = self.page.borrow().links().to_vec();
        for node in links {
            if let Some(place) = self.view.address(node).and_then(|binding| self.state.address(binding)) {
                self.page.borrow_mut().set_href(node, &places::bytes(place), &mut ops);
            }
        }
        self.dom.apply(&ops);
        let window = web_sys::window().expect("browser window");
        if let Some(timer) = self.timer.take() {
            window.clear_timeout_with_handle(timer);
        }
        if let Some(deadline) = self.state.deadline() {
            let millis = deadline.as_nanos().saturating_sub(now.as_nanos()).div_ceil(1_000_000);
            self.timer = Some(
                window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        self.timer_callback.as_ref().unchecked_ref(),
                        i32::try_from(millis).unwrap_or(i32::MAX),
                    )
                    .expect("view deadline"),
            );
        }
    }
}
fn no_drawing(_: skein_ui::DrawingKind, _: &[u8], _: u16, _: u16, _: &mut skein_ui::scene::Scene) -> bool {
    false
}
