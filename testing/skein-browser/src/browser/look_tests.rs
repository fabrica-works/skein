//! A fake CDP peer for the shot's document and caret steps.
use super::super::{Browser, Page, Phase};
use super::{Css, Look, Looked, Rect, Refusal};
use crate::{Limits, boundary::Event, wire::decode::Document};
use alloc::boxed::Box;
use skein_lib::{Env, Time, Token, Wall};

const PAGE: Token = Token::new(3);
const SHOT: Token = Token::new(4);
const FRAME: &[u8] = br#"{"frameTree":{"frame":{"id":"main","loaderId":"document-one"}}}"#;
const NEXT_FRAME: &[u8] = br#"{"frameTree":{"frame":{"id":"main","loaderId":"document-two"}}}"#;
const METRICS: &[u8] = br#"{"cssVisualViewport":{"pageX":0,"pageY":0,"clientWidth":390,"clientHeight":844}}"#;

struct Peer {
    browser: Browser,
    env: Env<Limits>,
}
impl Peer {
    fn new() -> Self {
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: Limits::default() };
        let mut browser = Browser::new(Token::new(1), &env.limits);
        browser.phase = Phase::Ready;
        assert!(browser.insert_page(
            Page {
                token: PAGE,
                person: Token::new(2),
                target: Some(Box::from(b"target".as_slice())),
                session: Some(Box::from(b"session".as_slice())),
                initial_url: Box::from(b"about:blank".as_slice()),
                frame: Some(Box::from(b"main".as_slice())),
                navigating: false,
                opening: false,
                load: None,
                load_due: None,
                closed: false,
                css: Css::Disabled,
                caret_document: None,
            },
            env.limits.pages
        ));
        Self { browser, env }
    }
    fn shot(&mut self, clip: Option<Rect>) {
        self.browser.request_look(&self.env, PAGE, SHOT, Look::Shot { clip, scale: 1 });
    }
    fn answer(&mut self, method: &[u8], params: &[u8], result: &[u8]) {
        self.reply(method, params, b"result", result);
    }
    fn reply(&mut self, method: &[u8], params: &[u8], field: &[u8], result: &[u8]) {
        let index = self.browser.sendable().expect("next command is sendable");
        let command = self.browser.outgoing.remove(index).expect("next command");
        let bytes = command.bytes.strip_suffix(&[0]).expect("CDP delimiter");
        let document = Document::parse(bytes, self.env.limits.command).expect("encoded command");
        assert_eq!(document.root().get(b"method").expect("method").text().expect("method text").as_ref(), method);
        assert_eq!(document.root().get(b"params").expect("parameters").raw(), params);
        // Keep the command's encoded id, so the fake answers through the wire decoder.
        let mut reply = Vec::new();
        reply.extend_from_slice(b"{\"id\":");
        reply.extend_from_slice(document.root().get(b"id").expect("id").raw());
        reply.extend_from_slice(b",\"");
        reply.extend_from_slice(field);
        reply.extend_from_slice(b"\":");
        reply.extend_from_slice(result);
        reply.extend_from_slice(b"}");
        self.browser.message(&self.env, &reply);
    }
    fn sheet(&mut self) {
        self.answer(b"CSS.createStyleSheet", br#"{"frameId":"main"}"#, br#"{"styleSheetId":"inspector"}"#);
        self.answer(
            b"CSS.setStyleSheetText",
            br#"{"styleSheetId":"inspector","text":"* { caret-animation: manual !important }"}"#,
            b"{}",
        );
    }
    fn captured(&mut self) {
        self.answer(
            b"Page.captureScreenshot",
            br#"{"format":"png","clip":{"x":0,"y":0,"width":390,"height":844,"scale":1},"captureBeyondViewport":true}"#,
            br#"{"data":"iVBORw0KGgo="}"#,
        );
        check_shot(self.browser.events.pop_front().expect("shot terminal"));
        assert!(self.browser.outgoing.is_empty());
    }
    fn viewport(&mut self) {
        self.answer(b"Page.getLayoutMetrics", b"{}", METRICS);
        self.captured();
    }
    fn first_shot(&mut self) {
        self.shot(None);
        self.answer(b"CSS.enable", b"{}", b"{}");
        self.answer(b"Page.getFrameTree", b"{}", FRAME);
        self.sheet();
        self.viewport();
    }
}
fn check_shot(event: Event) {
    let Event::Looked { op: SHOT, looked: Looked::Shot(png) } = event else { panic!("shot terminal") };
    assert_eq!(png.as_ref(), b"\x89PNG\r\n\x1a\n");
}
#[test]
fn first_viewport_shot_installs_caret_before_capture() {
    Peer::new().first_shot();
}
#[test]
fn shot_during_navigation_can_send_its_caret_setup() {
    let mut peer = Peer::new();
    peer.browser.page_mut(PAGE).expect("page").navigating = true;
    peer.first_shot();
}
#[test]
fn clipped_shot_installs_caret_and_reuses_enabled_css() {
    let mut peer = Peer::new();
    peer.browser.page_mut(PAGE).expect("page").css = Css::Enabled;
    peer.shot(Some(Rect { x: 0, y: 0, width: 390, height: 844 }));
    peer.answer(b"Page.getFrameTree", b"{}", FRAME);
    peer.sheet();
    peer.captured();
}
#[test]
fn same_document_keeps_its_caret_stylesheet() {
    let mut peer = Peer::new();
    peer.first_shot();
    peer.browser.message(
        &peer.env,
        br#"{"method":"Page.navigatedWithinDocument","sessionId":"session","params":{"frameId":"main"}}"#,
    );
    peer.shot(None);
    peer.answer(b"Page.getFrameTree", b"{}", FRAME);
    peer.viewport();
}
#[test]
fn navigation_installs_a_stylesheet_in_the_next_document() {
    let mut peer = Peer::new();
    peer.first_shot();
    peer.browser.message(&peer.env, br#"{"method":"Page.frameNavigated","sessionId":"session","params":{"frame":{"id":"main","loaderId":"document-two"}}}"#);
    peer.shot(None);
    peer.answer(b"Page.getFrameTree", b"{}", NEXT_FRAME);
    peer.sheet();
    peer.viewport();
}
#[test]
fn refused_caret_commands_end_the_shot_and_allow_a_retry() {
    for method in [b"CSS.createStyleSheet".as_slice(), b"CSS.setStyleSheetText".as_slice()] {
        let mut peer = Peer::new();
        peer.shot(None);
        peer.answer(b"CSS.enable", b"{}", b"{}");
        peer.answer(b"Page.getFrameTree", b"{}", FRAME);
        let params = if method == b"CSS.createStyleSheet" {
            br#"{"frameId":"main"}"#.as_slice()
        } else {
            peer.answer(b"CSS.createStyleSheet", br#"{"frameId":"main"}"#, br#"{"styleSheetId":"inspector"}"#);
            br#"{"styleSheetId":"inspector","text":"* { caret-animation: manual !important }"}"#.as_slice()
        };
        peer.reply(method, params, b"error", br#"{"code":-32000,"message":"refused"}"#);
        let Some(Event::Refused { op: SHOT, why: Refusal::Protocol }) = peer.browser.events.pop_front() else {
            panic!("shot refused")
        };
        assert!(peer.browser.outgoing.is_empty());
        assert!(peer.browser.page(PAGE).expect("page").caret_document.is_none());
        peer.shot(None);
        peer.answer(b"Page.getFrameTree", b"{}", FRAME);
        peer.sheet();
        peer.viewport();
    }
}

#[test]
fn document_identity_at_text_limit_is_whole_and_past_it_is_refused() {
    let mut peer = Peer::new();
    let length = u32::try_from(b"document-one".len()).expect("fixture length fits u32");
    peer.env.limits.text = length;
    peer.first_shot();
    peer.env.limits.text = length.checked_sub(1).expect("fixture is nonempty");
    peer.shot(None);
    peer.answer(b"Page.getFrameTree", b"{}", FRAME);
    let Some(Event::Refused { op: SHOT, why: Refusal::Limit }) = peer.browser.events.pop_front() else {
        panic!("document identity refused whole")
    };
    assert!(peer.browser.outgoing.is_empty());
}
