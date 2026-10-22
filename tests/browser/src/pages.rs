const BUTTON: &[u8] = br#"<!doctype html><html><body><button onclick="document.querySelector('h1').textContent='Done'">Press</button><h1>Before</h1></body></html>"#;
const FORM: &[u8] = br#"<!doctype html><html><body><form onsubmit="event.preventDefault();document.querySelector('h1').textContent='Hello '+document.querySelector('input').value"><label>Name <input aria-label="Name" value="Old words"></label><button type="submit">Submit</button></form><h1>Before</h1></body></html>"#;
const COVERED: &[u8] = br#"<!doctype html><html><body><button onclick="document.querySelector('h1').textContent='Wrong'">Covered</button><h1>Before</h1><div style="position:fixed;inset:0;background:#abc;opacity:.9;z-index:10"></div></body></html>"#;
const SCROLL: &[u8] = br#"<!doctype html><html><body><div style="height:2200px">Long page</div><button onclick="document.querySelector('h1').textContent='Scrolled'">Far away</button><h1>Before</h1></body></html>"#;
const THROW: &[u8] = br#"<!doctype html><html><body><button onclick="throw Error('test exception')">Throw</button><h1>Before</h1></body></html>"#;
const CSP: &[u8] = br#"<!doctype html><html><body><h1>CSP page</h1><script>document.querySelector('h1').textContent='Wrong'</script></body></html>"#;

const LEAVE: &[u8] = br#"<!doctype html><html><body><button onclick="window.onbeforeunload=event=>{event.preventDefault();event.returnValue='';};document.querySelector('h1').textContent='Held'">Hold</button><button onclick="document.querySelector('h1').textContent='Answer: '+confirm('Delete it?')">Ask</button><h1>Before</h1></body></html>"#;
const WORDS: &[u8] = br#"<!doctype html><html><body><p>Checks: 3 failing, 2 passed</p></body></html>"#;
const FIGURE: &[u8] = br#"<!doctype html><html><body><figure aria-label="Error rate"><svg aria-hidden="true" width="200" height="100"><rect id="bar" x="20" y="15" width="30" height="25" fill="blue" /></svg></figure><h1>Before</h1><script src="/press.js"></script></body></html>"#;
const PRESS: &[u8] = br#"document.querySelector('#bar').addEventListener('click', () => { document.querySelector('h1').textContent = 'Pressed shape'; });"#;

pub fn pages() -> Vec<skein_fake_person_browser::origins::Page> {
    use skein_fake_person_browser::origins::Page;
    [
        (b"/leave".as_slice(), LEAVE),
        (b"/button".as_slice(), BUTTON),
        (b"/form".as_slice(), FORM),
        (b"/covered".as_slice(), COVERED),
        (b"/scroll".as_slice(), SCROLL),
        (b"/throw".as_slice(), THROW),
        (b"/csp".as_slice(), CSP),
        (b"/words".as_slice(), WORDS),
        (b"/figure".as_slice(), FIGURE),
        (b"/press.js".as_slice(), PRESS),
        (b"/favicon.ico".as_slice(), b""),
    ]
    .into_iter()
    .map(|(path, body)| Page {
        path: path.into(),
        kind: if path == b"/favicon.ico" {
            Box::new([])
        } else if path == b"/press.js" {
            b"text/javascript; charset=utf-8".as_slice().into()
        } else {
            b"text/html; charset=utf-8".as_slice().into()
        },
        body: body.into(),
        headers: if path == b"/csp" {
            vec![(b"Content-Security-Policy".as_slice().into(), b"default-src 'none'".as_slice().into())]
        } else {
            vec![]
        },
    })
    .collect()
}
