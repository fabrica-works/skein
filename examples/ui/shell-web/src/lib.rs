//! The reference shell: start the parts, carry reports, apply the view's patches.
#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]
use skein_lib::{Queue, Writer};
use skein_ui::{Act, NodeId, Owner, View, look::Names};
use skein_ui_example_view::{Duties, Report as ViewReport, State, limits, places};
use skein_ui_web::{
    Look, fetch,
    inbox::Inbox,
    page::{Page, web::Dom},
    platform, wasm_bindgen,
};
use std::{cell::RefCell, rc::Rc};

const NAMES: &[Names<'_>] = &[Names { owner: Owner { number: 1 }, prefix: b"notes", styles: &[], regions: &[] }];
// Each part wraps its happening in the application's report type.
enum Report {
    Start,
    Act(NodeId, Act),
    Fetch(fetch::Report),
}
struct Shell {
    state: State,
    view: View,
    page: Rc<RefCell<Page<'static>>>,
    dom: Dom,
    fetch: fetch::web::Fetch<Report>,
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
    let state = State::start(facts.seed, path.as_bytes(), &[], facts.offset, &facts.languages);
    let limits = limits::limits();
    let inbox = Inbox::new();
    let page = Rc::new(RefCell::new(Page::new(
        Look { names: NAMES, pictures: &[], draw: no_drawing },
        &limits,
        b"notes",
        facts.primary,
    )));
    let dom = Dom::new(mount, &page, &inbox, Report::Act);
    let fetch = fetch::web::Fetch::new(8, Rc::clone(&inbox), Report::Fetch);
    let mut shell = Shell { state, view: View::new(&limits), page, dom, fetch };
    // The inbox keeps the shell and its listeners alive and forbids a turn inside a turn.
    inbox.start(Box::new(move |inbox| shell.turn(inbox)));
    inbox.raise(Report::Start);
}
impl Shell {
    fn turn(&mut self, inbox: &Inbox<Report>) {
        let (now, wall) = platform::clocks();
        while let Some(report) = inbox.next() {
            let report = match report {
                Report::Start => Some(ViewReport::Due),
                Report::Act(node, act) => self.view.act(node, act).map(ViewReport::Act),
                Report::Fetch(fetch::Report::Answered { exchange, status, body }) => {
                    Some(ViewReport::Answered { exchange, status, body })
                }
                Report::Fetch(fetch::Report::Failed { exchange }) => Some(ViewReport::Failed { exchange }),
            };
            let Some(report) = report else { continue };
            let mut duties = Duties {
                writes: Queue::with_capacity(8),
                removes: Queue::with_capacity(8),
                exchanges: Queue::with_capacity(8),
                show: None,
            };
            self.state.step(report, now, wall, &mut duties);
            // This thin view emits exchanges. The transport will carry its stored requests.
            assert!(duties.writes.is_empty() && duties.removes.is_empty(), "the thin view has no store duties");
            while let Some(exchange) = duties.exchanges.pop() {
                self.fetch.exchange(&fetch::Exchange {
                    id: exchange.id,
                    path: exchange.path,
                    method: if exchange.post { "POST" } else { "GET" }.into(),
                    headers: vec![],
                    body: exchange.body,
                    bound: exchange.bound,
                });
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
                let mut path = Writer::new(27);
                places::print(place, &mut path);
                self.page.borrow_mut().set_href(node, &path.finish(), &mut ops);
            }
        }
        self.dom.apply(&ops);
    }
}
fn no_drawing(_: skein_ui::DrawingKind, _: &[u8], _: u16, _: u16, _: &mut skein_ui::scene::Scene) -> bool {
    false
}
