//! The actual accounts owner and independent issuer over shared seeded wires.
use super::browser::url_parts;
use super::{Client, Fact, ROOM, Story, confidential, registration};
use skein_fake_oauth as fake;
use skein_io as io;
use skein_lib::{Duration, Env, Queue, Time, Wall, bytes};
use skein_oauth as oauth;
use skein_oauth_accounts as accounts;
use std::net::Ipv4Addr;
/// The same owner runs directly over seeded protocol streams and fake issuer.
#[must_use]
#[expect(clippy::too_many_lines, reason = "the protocol loop exposes every owner and stream boundary")]
pub fn protocol(seed: u64, story: Story) -> (Vec<Fact>, Vec<String>) {
    use skein_http::server as http;
    use skein_lib::Token;
    use skein_lib::stream::{Down, Read, Up};
    use skein_world::stream::Wire;

    let mut profile = crate::world::limits();
    if story == Story::TwoPorts {
        profile.listeners = 2;
        profile.io.sockets = 6;
    }
    if matches!(story, Story::Return(_)) {
        profile.server.response = profile.server.response.max(4096);
    }
    let registration = registration(story);
    let issuer_limits = fake::Limits {
        codes: if matches!(story, Story::TwoPorts | Story::OpenIdAgain | Story::Return(super::Return::After)) {
            2
        } else {
            1
        },
        carried: 3,
        ..crate::world::issuer_limits(&profile.client)
    };
    let mut issuer = fake::Issuer::new(
        fake::Config {
            authorization_url: registration.authorization_url.clone(),
            token_endpoint: registration.token_endpoint.clone(),
            client_id: registration.client_id.clone(),
            client_secret: registration.client_secret.clone(),
            redirect_uri: registration.redirect_uri.clone(),
            refresh_token: bytes::copy_of(b"seed"),
            terms: super::issuer::port_terms(story),
        },
        issuer_limits,
    )
    .expect("issuer");
    for (access, refresh) in [(b'a', b"r1".as_slice()), (b'b', b"r2".as_slice())] {
        issuer
            .queue(fake::Plan {
                status: 200,
                body: fake::Body::Token(oauth::TokenResponse {
                    access_token: vec![if story == Story::TwoPorts { b'a' } else { access }; 16].into_boxed_slice(),
                    refresh_token: Some(bytes::copy_of(refresh)),
                    expires_in: 30,
                    scope: (story == Story::RequiredScopes).then(|| bytes::copy_of(b"read")),
                    id_token: None,
                    earliest_refresh_at: None,
                }),
                delay: Duration::from_millis(5),
                retry_after: Duration::ZERO,
            })
            .expect("plan");
    }
    let server_limits = http::Limits {
        head: 2048,
        headers: 16,
        body: 1024,
        read: 256,
        response: skein_fake_peers::oauth::least_response(&issuer_limits)
            .expect("world issuer response floor fits u32"),
        send: 256,
    };
    let mut server = http::Server::new(&server_limits);
    let mut server_events = Queue::with_capacity(ROOM);
    let mut server_requests = Queue::with_capacity(ROOM);
    let mut server_down = Queue::with_capacity(ROOM);
    let mut issuer_out = Queue::with_capacity(ROOM);
    let mut client_wire = Wire::new(seed);
    let mut server_wire = Wire::new(seed.wrapping_add(1));
    let mut client = Client::new(story, false);
    client.address = Some((Ipv4Addr::LOCALHOST, 31000).into());
    let mut now = Time::ZERO;
    client.start(&Env { now, wall: Wall::EPOCH, limits: profile });
    let mut owner = None;
    let mut body = Vec::new();
    let mut document = None;
    let mut document_paths = Vec::new();
    let mut response: Option<(Box<[u8]>, usize)> = None;
    let mut server_closed = false;
    let mut listeners = Vec::new();
    let mut preferred_failed = false;
    let mut callback = None;
    let mut callback_wire = Wire::new(seed.wrapping_add(2));
    let mut browser_bytes = Vec::new();
    let mut redirect = None;
    let mut callback_count = 0;
    let mut scripted = 0;
    let mut callback_pending = false;
    let mut trace = Vec::new();

    for _ in 0..200_000 {
        let wall = Wall::from_nanos(now.as_nanos());
        let env = Env { now, wall, limits: profile };
        let server_env = Env { now, wall, limits: server_limits };
        client.restart(&env);
        let component = client.component.as_mut().expect("component");
        if component.has_work() || component.next_deadline().is_some_and(|due| due <= now) {
            component.fire(&env, &mut client.above, &mut client.requests, &mut client.files);
        }
        client.observe(&env);
        if (scripted == 0 || client.facts.contains(&Fact::Granted(1)))
            && let Some(url) = client.visits.get(scripted)
        {
            scripted += 1;
            if !matches!(story, Story::Cancel | Story::Abort | Story::Timeout) {
                issuer.step(fake::Event::Authorize { url: url.clone(), now }, &mut issuer_out);
                let Some(fake::Request::Redirect { uri, state, code, client_id }) = issuer_out.pop() else {
                    panic!("authorized fake browser")
                };
                let mut location = uri.to_vec();
                location.extend_from_slice(b"?state=");
                location.extend_from_slice(&state);
                location.extend_from_slice(b"&code=");
                location.extend_from_slice(&code);
                if let Some(client_id) = client_id {
                    location.extend_from_slice(b"&client_id=");
                    location.extend_from_slice(&client_id);
                }
                redirect = Some(location.into_boxed_slice());
            }
        }
        if !callback_pending
            && callback.is_none()
            && callback_count
                < if matches!(
                    story,
                    Story::WrongPath
                        | Story::Return(super::Return::WrongPath | super::Return::After)
                        | Story::WrongHost
                        | Story::LongHead
                        | Story::TwoPorts
                        | Story::OpenIdAgain
                ) {
                    2
                } else {
                    1
                }
            && let Some(uri) = &redirect
        {
            if confidential(story) {
                client.ask(&env, accounts::Request::Redirected { account: 0, uri: uri.clone() });
                redirect = None;
            } else if let Some((identity, _, _)) =
                listeners.iter().find(|(_, _, port)| *port == url_parts(uri).0.port()).copied()
            {
                client.component.as_mut().expect("component").up(
                    &env,
                    io::Event::Accepted {
                        owner: identity,
                        socket: Token::new(11),
                        peer: io::Addr::Ip((Ipv4Addr::LOCALHOST, 31235).into()),
                    },
                    &mut client.above,
                    &mut client.requests,
                    &mut client.files,
                );
                let (_, authority, target) = url_parts(uri);
                let target = if callback_count == 0
                    && matches!(story, Story::WrongPath | Story::Return(super::Return::WrongPath))
                {
                    b"/wrong?state=fake&code=fake".as_slice()
                } else {
                    target
                };
                let authority = if callback_count == 0 && story == Story::WrongHost {
                    b"127.0.0.1:31234".as_slice()
                } else {
                    authority
                };
                let mut head = b"GET ".to_vec();
                head.extend_from_slice(target);
                if callback_count == 0 && story == Story::LongHead {
                    head.extend_from_slice(&vec![b'x'; 3000]);
                }
                head.extend_from_slice(b" HTTP/1.1\r\nHost: ");
                head.extend_from_slice(authority);
                head.extend_from_slice(b"\r\nConnection: close\r\n\r\n");
                callback_wire = Wire::new(seed.wrapping_add(2 + callback_count));
                callback_wire.write(&head);
                browser_bytes.clear();
                callback_count += 1;
                callback_pending = true;
                if matches!(story, Story::TwoPorts | Story::OpenIdAgain | Story::Return(super::Return::After)) {
                    redirect = None;
                }
            }
        }
        if let Some(request) = client.requests.pop() {
            match request {
                io::Request::Connect { owner: identity, .. } => {
                    assert!(owner.replace(identity).is_none(), "one exchange socket");
                    let component = client.component.as_mut().expect("component");
                    component.up(
                        &env,
                        io::Event::Connecting { owner: identity, socket: Token::new(9) },
                        &mut client.above,
                        &mut client.requests,
                        &mut client.files,
                    );
                    component.up(
                        &env,
                        io::Event::Connected { owner: identity },
                        &mut client.above,
                        &mut client.requests,
                        &mut client.files,
                    );
                    server = http::Server::new(&server_limits);
                    server_closed = false;
                    server_events = Queue::with_capacity(ROOM);
                    server_requests = Queue::with_capacity(ROOM);
                    server_down = Queue::with_capacity(ROOM);
                    client_wire = Wire::new(seed.wrapping_add(4 + issuer.posts()));
                    server_wire = Wire::new(seed.wrapping_add(5 + issuer.posts()));
                    body.clear();
                    response = None;
                    server_requests.push(http::Request::Next);
                }
                io::Request::Listen { owner: identity, addr } => {
                    let io::Addr::Ip(mut address) = addr else { panic!("IP listener") };
                    if story == Story::VaryingTaken && address.port() == 31234 {
                        preferred_failed = true;
                        let component = client.component.as_mut().expect("component");
                        component.up(
                            &env,
                            io::Event::Failed { owner: identity, error: io::Error::Other },
                            &mut client.above,
                            &mut client.requests,
                            &mut client.files,
                        );
                        component.up(
                            &env,
                            io::Event::Closed { owner: identity },
                            &mut client.above,
                            &mut client.requests,
                            &mut client.files,
                        );
                    } else {
                        if address.port() == 0 {
                            address.set_port(41000 + u16::try_from(listeners.len()).expect("two listeners"));
                        }
                        let token = Token::new(20 + u64::try_from(listeners.len()).expect("two listeners"));
                        listeners.push((identity, token, address.port()));
                        client.component.as_mut().expect("component").up(
                            &env,
                            io::Event::Listening { owner: identity, listener: token, addr: io::Addr::Ip(address) },
                            &mut client.above,
                            &mut client.requests,
                            &mut client.files,
                        );
                    }
                }
                io::Request::Bind { owner: identity, socket } => {
                    assert_eq!(socket, Token::new(11));
                    assert!(callback.replace(identity).is_none());
                }
                io::Request::Stream { stream, down } if stream == Token::new(11) => {
                    if let Down::Send(bytes) = &down {
                        browser_bytes.extend_from_slice(bytes);
                    }
                    callback_wire.take(down);
                }
                io::Request::Stream { down, .. } => {
                    match &down {
                        Down::Send(bytes) => server_wire.write(bytes),
                        Down::Finish => server_wire.eof = true,
                        Down::Demand { .. } => {}
                    }
                    client_wire.take(down);
                }
                io::Request::Close { entity } | io::Request::Abort { entity } => {
                    let identity = if entity == Token::new(9) {
                        trace.append(&mut client_wire.trace);
                        trace.append(&mut server_wire.trace);
                        server_requests.push(http::Request::Close);
                        owner.take().expect("web owner")
                    } else if let Some(at) = listeners.iter().position(|(_, token, _)| *token == entity) {
                        listeners.remove(at).0
                    } else {
                        assert_eq!(entity, Token::new(11));
                        if !matches!(story, Story::Cancel | Story::Abort | Story::Timeout) {
                            assert!(browser_bytes.starts_with(if callback_count == 1 && story == Story::LongHead {
                                b"HTTP/1.1 414"
                            } else if callback_count == 1
                                && matches!(
                                    story,
                                    Story::WrongPath | Story::WrongHost | Story::Return(super::Return::WrongPath)
                                )
                            {
                                b"HTTP/1.1 404"
                            } else if super::return_location(story).is_some()
                                || (story == Story::Return(super::Return::After) && callback_count == 1)
                            {
                                b"HTTP/1.1 303"
                            } else {
                                b"HTTP/1.1 200"
                            }));
                        }
                        trace.append(&mut callback_wire.trace);
                        callback_pending = false;
                        callback.take().expect("callback owner")
                    };
                    client.component.as_mut().expect("component").up(
                        &env,
                        io::Event::Closed { owner: identity },
                        &mut client.above,
                        &mut client.requests,
                        &mut client.files,
                    );
                }
                io::Request::Reject { .. }
                | io::Request::Output { .. }
                | io::Request::Spawn { .. }
                | io::Request::Signal { .. }
                | io::Request::Window { .. }
                | io::Request::Usage { .. } => panic!("only admitted sockets"),
            }
        }
        if let Some(identity) = callback
            && let Some(up) = callback_wire.answer()
        {
            client.component.as_mut().expect("component").up(
                &env,
                io::Event::Stream { owner: identity, up },
                &mut client.above,
                &mut client.requests,
                &mut client.files,
            );
        }
        if let Some(identity) = owner
            && let Some(up) = client_wire.answer()
        {
            client.component.as_mut().expect("component").up(
                &env,
                io::Event::Stream { owner: identity, up },
                &mut client.above,
                &mut client.requests,
                &mut client.files,
            );
        }
        if let Some(request) = server_requests.pop() {
            http::down(&mut server, &server_env, request, &mut server_events, &mut server_down);
        }
        if !server_closed && let Some(up) = server_wire.answer() {
            http::up(&mut server, &server_env, up, &mut server_events, &mut server_down);
        }
        if let Some(down) = server_down.pop() {
            match &down {
                Down::Send(bytes) => client_wire.write(bytes),
                Down::Finish => client_wire.eof = true,
                Down::Demand { .. } => {}
            }
            server_wire.take(down);
        }
        if let Some(event) = server_events.pop() {
            match event {
                http::Event::Call(call) => {
                    if call.method == skein_http::Method::Get {
                        document_paths.push(call.target.clone());
                        let mut uri = super::ISSUER.to_vec();
                        uri.extend_from_slice(&call.target);
                        document = Some(uri.into_boxed_slice());
                    } else {
                        assert_eq!(call.method, skein_http::Method::Post);
                        assert_eq!(call.target.as_ref(), b"/token");
                        assert_eq!(call.header(b"content-type"), Some(b"application/x-www-form-urlencoded".as_slice()));
                    }
                    server_requests.push(http::Request::Body(Down::Demand {
                        read: Read::Arrived {
                            max: profile
                                .client
                                .document
                                .document_bytes
                                .saturating_sub(u32::try_from(body.len()).expect("bounded request body"))
                                .max(1),
                        },
                        room: 0,
                    }));
                }
                http::Event::Body(Up::Bytes(bytes)) => {
                    body.extend_from_slice(&bytes);
                    server_requests.push(http::Request::Body(Down::Demand {
                        read: Read::Arrived {
                            max: profile
                                .client
                                .document
                                .document_bytes
                                .saturating_sub(u32::try_from(body.len()).expect("bounded request body"))
                                .max(1),
                        },
                        room: 0,
                    }));
                }
                http::Event::Body(Up::End) if document.is_some() => issuer.step(
                    fake::Event::Get { id: 0, endpoint: document.take().expect("document URL"), now, wall },
                    &mut issuer_out,
                ),
                http::Event::Body(Up::End) => issuer.step(
                    fake::Event::Post {
                        request: oauth::HttpRequest {
                            method: oauth::Method::Post,
                            id: 0,
                            endpoint: registration.token_endpoint.clone(),
                            content_type: b"application/x-www-form-urlencoded",
                            body: bytes::copy_of(&body),
                            deadline: now.saturating_add(Duration::from_secs(10)),
                        },
                        now,
                        wall,
                    },
                    &mut issuer_out,
                ),
                http::Event::Reply(Up::Room) => {
                    let (bytes, offset) = response.as_mut().expect("response body");
                    let end = offset.saturating_add(256).min(bytes.len());
                    server_requests.push(http::Request::Reply(Down::Send(bytes::copy_of(&bytes[*offset..end]))));
                    *offset = end;
                    if end == bytes.len() {
                        server_requests.push(http::Request::Reply(Down::Finish));
                    } else {
                        server_requests.push(http::Request::Reply(Down::Demand {
                            read: Read::Nothing,
                            room: u32::try_from(bytes.len() - end).expect("bounded body").min(256),
                        }));
                    }
                }
                http::Event::Closed => server_closed = true,
                http::Event::Done(_) => {}
                other @ (http::Event::Ended
                | http::Event::Body(_)
                | http::Event::Reply(_)
                | http::Event::Refused(_)
                | http::Event::Failed(_)) => panic!("positive independent issuer HTTP event: {other:?}"),
            }
        }
        issuer.step(fake::Event::Tick { now, wall }, &mut issuer_out);
        if let Some(output) = issuer_out.pop() {
            match output {
                fake::Request::Http(answer) => {
                    server_requests.push(http::Request::Respond(http::Response {
                        status: answer.status,
                        headers: Box::new([]),
                        body: http::Body::Length(u64::try_from(answer.body.len()).expect("bounded response")),
                        close: true,
                    }));
                    server_requests.push(http::Request::Reply(Down::Demand {
                        read: Read::Nothing,
                        room: u32::try_from(answer.body.len()).expect("bounded response").min(256),
                    }));
                    response = Some((answer.body, 0));
                }
                fake::Request::Redirect { .. } | fake::Request::Refused => panic!("valid rotating refresh"),
            }
        }
        if client.closed
            && client.requests.is_empty()
            && client.above.is_empty()
            && owner.is_none()
            && listeners.is_empty()
            && callback.is_none()
        {
            assert_eq!(
                issuer.posts(),
                if matches!(
                    story,
                    Story::Public | Story::TwoPorts | Story::OpenIdAgain | Story::Return(super::Return::After)
                ) {
                    2
                } else {
                    u64::from(!matches!(story, Story::Cancel | Story::Abort | Story::Timeout))
                }
            );
            assert_eq!(preferred_failed, story == Story::VaryingTaken);
            if super::again(story) {
                assert_eq!(
                    document_paths.iter().map(AsRef::as_ref).collect::<Vec<&[u8]>>(),
                    [b"/.well-known/openid-configuration".as_slice(), b"/keys"].repeat(2)
                );
            }
            trace.append(&mut client_wire.trace);
            trace.append(&mut server_wire.trace);
            trace.append(&mut callback_wire.trace);
            return (client.facts, trace);
        }
        now = now.saturating_add(Duration::from_millis(1));
    }
    panic!("protocol world settles");
}
