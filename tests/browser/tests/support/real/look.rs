//! Facts on the lab's reads, served and driven by the real loop.
#![expect(clippy::disallowed_types, reason = "the kit's own tests of the look's reads (browser.md, 5.1)")]
use super::{Event, PAGE, Request, Rig, Stage, Token, fonts, query, serve};
use skein_browser::boundary::{Go, Look, Looked, Motion, Rect, Scheme};
use skein_lib::{Duration, List};
const READ: Token = Token::new(40);
const GO: Token = Token::new(41);
const READY: Token = Token::new(42);
const FIND: Token = Token::new(43);
const EXPECTED: Rect = Rect { x: 20, y: 2000, width: 120, height: 40 };
const FIXTURE: &[u8] = br#"<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1">
<style>
@font-face {font-family:Fixture;src:url('/fixture.ttf')}
body {margin:0;color:rgb(17,17,17);font-size:16px}
#media {display:none}
@media (prefers-color-scheme:dark) and (prefers-reduced-motion:reduce) and (width:390px) { #media {display:block} }
#styled,#fallback {position:absolute;left:20px;top:2000px;box-sizing:border-box;width:120px;height:40px;border:3px solid black;padding:0;font:16px Fixture;color:rgb(17,17,17)}
#fallback {top:2060px;font-family:monospace}
</style></head><body>
<h1 id="media">Dark reduced 390</h1><div><h2>Second</h2></div><h3>Third</h3>
<input aria-label="Words" value="Old words" autofocus><textarea aria-label="Lines">Several lines</textarea>
<input type="checkbox" aria-label="Checked" checked><button disabled>Disabled</button>
<div id="nobox" style="display:none">Hidden</div><button id="styled">Styled</button><button id="fallback">Fallback<div><span style="font-family:Fixture">Nested</span></div></button>
<script>document.fonts.load('16px Fixture').then(()=>document.fonts.ready).then(()=>{let p=document.createElement('p');p.setAttribute('role','status');p.setAttribute('aria-label','Fonts ready');p.textContent='Fonts ready';document.body.append(p);window.scrollTo(0,100)})</script>
</body></html>"#;
#[derive(Default)]
pub(super) struct State {
    step: u8,
    node: u64,
    rect: Option<Rect>,
}
impl Rig {
    pub fn look_fixture(&mut self) {
        self.fixed_page(FIXTURE.to_vec());
        self.pages.pages.push(serve::Page {
            path: b"/fixture.ttf".as_slice().into(),
            kind: b"font/ttf".as_slice().into(),
            body: std::fs::read(fonts().join("DejaVuSans.ttf")).expect("read fixture font").into(),
            headers: vec![],
        });
    }
    fn look_ask(&mut self, step: u8, look: Look) {
        self.look.step = step;
        self.ask(Request::Look { page: PAGE, op: READ, look });
    }
    fn style_ask(&mut self, step: u8) {
        let mut properties = List::with_capacity(2);
        properties.push(Box::from(b"color".as_slice())).unwrap();
        properties.push(Box::from(b"font-size".as_slice())).unwrap();
        self.look_ask(step, Look::Style { node: self.look.node, properties });
    }
    pub(super) fn look_event(&mut self, event: Event) -> Result<(), Event> {
        match event {
            Event::Opened { page: PAGE } => {
                self.stage = Stage::Finding;
                self.look_ask(
                    0,
                    Look::Viewport {
                        width: 390,
                        height: 844,
                        mobile: true,
                        scheme: Scheme::Dark,
                        motion: Motion::Reduce,
                    },
                );
            }
            Event::Looked { op: READ, looked: Looked::Viewport } => {
                assert_eq!(self.look.step, 0);
                let url = format!("http://{}/facts", self.pages.addr.expect("page server"));
                self.ask(Request::Go { page: PAGE, op: GO, to: Go::Address(url.into_bytes().into()) });
            }
            Event::Done { op: GO } => {
                self.ask(Request::Await {
                    page: PAGE,
                    op: READY,
                    query: query(b"status", b"Fonts ready"),
                    expect: skein_browser::boundary::Expect::Present,
                    within: Duration::from_secs(3),
                });
            }
            Event::Met { op: READY, .. } => {
                self.ask(Request::Find { page: PAGE, op: FIND, query: query(b"heading", b"Dark reduced 390") })
            }
            Event::Found { op: FIND, seen, more } => {
                assert_eq!((seen.len(), more), (1, 0), "viewport and media persist across navigation");
                if self.look.step == 0 {
                    self.look_ask(1, Look::Tree);
                } else {
                    self.look.node = seen.get(0).unwrap().node;
                    self.look_ask(3, Look::Select { selector: b"#styled".as_slice().into() });
                }
            }
            Event::Looked { op: READ, looked: Looked::Tree(entries) } => {
                assert_eq!(self.look.step, 1);
                let entries = entries.as_slice();
                assert_eq!((entries[0].depth, entries[0].role.as_ref()), (0, b"RootWebArea".as_slice()));
                let headings: Vec<_> = entries.iter().filter(|e| e.role.as_ref() == b"heading").collect();
                assert_eq!(
                    headings.iter().map(|e| (e.name.as_ref(), e.level)).collect::<Vec<_>>(),
                    [(b"Dark reduced 390".as_slice(), 1), (b"Second", 2), (b"Third", 3)]
                );
                assert_eq!(
                    headings.iter().map(|e| e.depth).collect::<Vec<_>>(),
                    [1, 1, 1],
                    "ignored body and wrapper children lifted"
                );
                let named =
                    |name: &[u8]| entries.iter().find(|entry| entry.name.as_ref() == name).expect("named tree entry");
                let field = named(b"Words");
                assert_eq!(field.value.as_ref(), b"Old words");
                assert!(field.states.focused);
                assert!(!field.multiline);
                let area = named(b"Lines");
                assert_eq!(area.value.as_ref(), b"Several lines");
                assert!(area.multiline);
                assert!(!area.states.focused);
                assert!(named(b"Checked").states.checked);
                assert!(named(b"Disabled").states.disabled);
                self.look.step = 2;
                self.ask(Request::Find { page: PAGE, op: FIND, query: query(b"button", b"Styled") });
            }
            Event::Looked { op: READ, looked: Looked::Selected(selected) } => match self.look.step {
                3 => {
                    assert_eq!(selected, Some(self.look.node));
                    self.look_ask(4, Look::Select { selector: b".absent".as_slice().into() });
                }
                4 => {
                    assert_eq!(selected, None);
                    self.style_ask(5);
                }
                9 => {
                    self.look.node = selected.expect("styled after shot");
                    self.style_ask(10);
                }
                12 => {
                    self.look.node = selected.expect("hidden node exists");
                    self.style_ask(13);
                }
                6 => {
                    self.look.node = selected.expect("fallback button");
                    self.style_ask(7);
                }
                other => panic!("unexpected selection at {other}"),
            },
            Event::Looked { op: READ, looked: Looked::Style { values, faces, rect } } => {
                assert_eq!(
                    values.iter().map(|p| p.value.as_ref()).collect::<Vec<_>>(),
                    [b"rgb(17, 17, 17)".as_slice(), b"16px"]
                );
                match self.look.step {
                    5 => {
                        assert_eq!(rect, Some(EXPECTED));
                        assert!(faces.iter().any(|face| face.custom && face.glyphs > 0), "web font: {faces:?}");
                        self.look.rect = rect;
                        self.look_ask(6, Look::Select { selector: b"#fallback".as_slice().into() });
                    }
                    7 => {
                        assert!(faces.iter().any(|face| !face.custom && face.glyphs == 8), "fallback face: {faces:?}");
                        assert!(faces.iter().any(|face| face.custom && face.glyphs == 6), "nested web font: {faces:?}");
                        self.look_ask(8, Look::Shot { clip: self.look.rect, scale: 2 });
                    }
                    13 => {
                        assert_eq!(rect, None);
                        self.close_browser();
                    }
                    10 => {
                        assert_eq!(rect, self.look.rect, "captureBeyondViewport must preserve layout");
                        self.look_ask(11, Look::Shot { clip: None, scale: 1 });
                    }
                    other => panic!("unexpected style at {other}"),
                }
            }
            Event::Looked { op: READ, looked: Looked::Shot(png) } => {
                assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
                assert_eq!(&png[12..16], b"IHDR");
                let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
                let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
                match self.look.step {
                    8 => {
                        assert_eq!((width, height), (240, 80));
                        self.look_ask(9, Look::Select { selector: b"#styled".as_slice().into() });
                    }
                    11 => {
                        assert_eq!((width, height), (390, 844));
                        self.look_ask(12, Look::Select { selector: b"#nobox".as_slice().into() });
                    }
                    other => panic!("unexpected shot at {other}"),
                }
            }
            other => return Err(other),
        }
        Ok(())
    }
}
