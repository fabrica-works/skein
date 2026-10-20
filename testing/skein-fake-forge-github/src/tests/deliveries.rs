use super::{fixtures as f, wall};
use crate::{
    delivery::Endpoint,
    service::{Outcome, Request, Service},
};
use skein_forge::github::{
    self,
    delivery::{Account, Change, Ci, Delivery, Pull},
    document::{Document, Kind, Row},
    log,
};
use skein_lib::{Duration, List, ReplyTo, Token, Writer};
fn request(
    service: &mut Service,
    target: &[u8],
    validator: Option<&[u8]>,
    now: skein_lib::Wall,
) -> crate::service::Response {
    let jwt = f::authorization(super::app().key, 1000, 1600);
    let Outcome::Reply(reply) = service.request(
        &Request {
            method: github::request::Method::Get,
            target,
            authorization: Some(jwt.expose()),
            version: Some(github::pin::API_VERSION.as_bytes()),
            agent: true,
            body: b"",
            validator,
        },
        now,
        ReplyTo::new(Token::new(1)),
    ) else {
        panic!("local log response")
    };
    reply
}
fn rows(reply: &crate::service::Response) -> List<log::Row> {
    assert_eq!(reply.status, 200);
    let mut bounds = f::limits().github;
    bounds.inner = 16;
    let (rows, doc) = f::decode_body(Kind::LogRows, &reply.body, &bounds);
    assert_eq!(doc, Document::Rows);
    let mut result = List::with_capacity(100);
    for row in &rows {
        let Row::Log(row) = row else { panic!("log row") };
        result.push(row.clone()).unwrap();
    }
    result
}
fn values() -> List<Delivery> {
    let mut values = List::with_capacity(16);
    for value in [
        Delivery::Other,
        Delivery::Push {
            repository: 2,
            branch: f::boxed(b"main"),
            after: crate::reads::commit(1),
            deleted: false,
            sender: 9,
        },
        Delivery::Pull { repository: 2, number: 3, action: Pull { action: f::boxed(b"enqueued"), reason: None } },
        Delivery::Pull {
            repository: 2,
            number: 3,
            action: Pull { action: f::boxed(b"dequeued"), reason: Some(f::boxed(b"CI_FAILURE")) },
        },
    ] {
        values.push(value).unwrap();
    }
    for event in [Ci::ChecksRequested, Ci::Destroyed] {
        values.push(Delivery::Ci { repository: 2, commit: crate::reads::commit(1), event }).unwrap();
    }
    for change in [
        Change::Created,
        Change::Deleted,
        Change::Suspended,
        Change::Unsuspended,
        Change::Repositories,
        Change::Renamed { login: f::boxed(b"acme") },
    ] {
        values
            .push(Delivery::Installation {
                installation: 1,
                account: Account { id: 22, login: f::boxed(b"acme") },
                change,
            })
            .unwrap();
    }
    values
}
#[test]
fn github_fake_delivery_sources_render_signed_payloads_and_omit_global_ids() {
    let mut runs = List::with_capacity(2);
    for _ in 0_u32..2 {
        let mut config = f::config();
        config.deliveries.endpoint =
            Some(Endpoint { host: f::boxed(b"127.0.0.1:8080"), target: f::boxed(b"/hooks/1") });
        let mut limits = f::limits();
        limits.github.inner = 16;
        let mut service = Service::new(config, &limits, 17).unwrap();
        let mut seen = List::with_capacity(16);
        for value in &values() {
            let (id, post) = service.release(value.clone(), Some(1), wall(1200)).unwrap();
            let post = post.unwrap();
            assert!((1_000_000_000_000_000_000..=9_999_999_999_999_999_999).contains(&id));
            let signature = github::delivery::sign(super::app().webhook.expose(), &post.body);
            assert!(github::delivery::verify(super::app().webhook.expose(), &post.body, &signature));
            assert!(skein_lib::bytes::find(&post.head, &signature).is_some());
            seen.push((id, post.head, post.body)).unwrap();
        }
        let listed = rows(&request(&mut service, b"/app/hook/deliveries?per_page=16", None, wall(1203)));
        assert_eq!(listed.len(), 12);
        for row in &listed {
            if [b"ping".as_slice(), b"installation", b"installation_repositories"].contains(&row.event.as_ref()) {
                assert_eq!((row.installation_id, row.repository_id), (None, None));
            }
            let mut target = Writer::new(128);
            target.put(b"/app/hook/deliveries/").unwrap();
            target.put(skein_lib::Decimal::of(row.id).as_bytes()).unwrap();
            let target = crate::answers::finish(target).unwrap();
            let reply = request(&mut service, &target, None, wall(1203));
            let (_, doc) = f::decode_with(
                github::decode::log_delivery_decoder(&row.event, &f::limits().github),
                &reply.body,
                &f::limits().github,
            );
            let Document::Delivery(logged) = doc else { panic!("logged payload") };
            for (id, _, body) in &seen {
                if *id == row.id {
                    assert_eq!(
                        github::delivery::decode(&logged.event, body, &f::limits().github).unwrap(),
                        logged.delivery
                    );
                }
            }
        }
        runs.push(seen).unwrap();
    }
    assert_eq!(runs.get(0), runs.get(1));
}
#[test]
fn github_fake_log_pages_visibility_retention_and_validator() {
    let mut config = f::config();
    config.deliveries.log_entries = 3;
    config.deliveries.log_min = Duration::from_secs(2);
    config.deliveries.log_max = Duration::from_secs(2);
    config.deliveries.log_keep = Duration::from_secs(10);
    let mut service = Service::new(config, &f::limits(), 19).unwrap();
    let initial = request(&mut service, b"/app/hook/deliveries?per_page=1", None, wall(1200));
    assert!(rows(&initial).is_empty());
    let etag = initial.etag.unwrap();
    let (first, post) = service.release(Delivery::Other, None, wall(1200)).unwrap();
    assert!(post.is_none());
    assert_eq!(request(&mut service, b"/app/hook/deliveries?per_page=1", Some(&etag), wall(1201)).status, 304);
    let changed = request(&mut service, b"/app/hook/deliveries?per_page=1", Some(&etag), wall(1202));
    let listed = rows(&changed);
    assert_eq!(listed.get(0).unwrap().id, first);
    assert_eq!(listed.get(0).unwrap().status_code, 502);
    assert_eq!(listed.get(0).unwrap().delivered_at, wall(1200));
    let etag = changed.etag.unwrap();
    let hit = request(&mut service, b"/app/hook/deliveries?per_page=1", Some(&etag), wall(1202));
    assert_eq!(hit.status, 304);
    assert!(hit.body.is_empty());
    let (second, _) = service.redeliver(first, wall(1202)).unwrap();
    let page = request(&mut service, b"/app/hook/deliveries?per_page=1", None, wall(1204));
    assert_eq!(rows(&page).get(0).unwrap().id, second);
    assert!(rows(&page).get(0).unwrap().redelivery);
    let link = page.link.unwrap();
    let start = skein_lib::bytes::find(&link, b"?").unwrap();
    let end = skein_lib::bytes::find(&link, b">").unwrap();
    let mut target = Writer::new(256);
    target.put(b"/app/hook/deliveries").unwrap();
    target.put(&link[start..end]).unwrap();
    let target = crate::answers::finish(target).unwrap();
    assert_eq!(rows(&request(&mut service, &target, None, wall(1204))).get(0).unwrap().id, first);
    for _ in 0_u32..8 {
        service.release(Delivery::Other, None, wall(1204)).unwrap();
    }
    assert_eq!(rows(&request(&mut service, b"/app/hook/deliveries?per_page=8", None, wall(1206))).len(), 3);
    assert!(rows(&request(&mut service, &target, None, wall(1206))).is_empty());
    assert!(rows(&request(&mut service, b"/app/hook/deliveries?per_page=8", None, wall(1214))).is_empty());
}
#[test]
fn github_fake_delivery_log_refuses_after_key_revocation() {
    let mut service = Service::new(f::config(), &f::limits(), 1).unwrap();
    let (id, _) = service.release(Delivery::Other, None, wall(1200)).unwrap();
    service.revoke_key();
    let target = github::request::encode(
        &github::request::Request::LogDelivery { id, event: Box::new([]) },
        &f::limits().github,
    )
    .unwrap()
    .target;
    for target in [f::boxed(b"/app/hook/deliveries?per_page=1"), target] {
        assert_eq!(request(&mut service, &target, None, wall(1203)).status, 401);
    }
}

#[test]
fn github_fake_delivery_choices_are_checked_before_allocation() {
    for case in 0_u32..6 {
        let mut config = f::config();
        match case {
            0 => config.deliveries.log_entries = 0,
            1 => config.deliveries.log_entries = 65,
            2 => config.deliveries.log_min = Duration::from_secs(3),
            3 => config.deliveries.payload_size = 32769,
            4 => {
                config.deliveries.endpoint =
                    Some(Endpoint { host: f::boxed(b"192.0.2.1:8080"), target: f::boxed(b"/hooks/1") });
            }
            5 => {
                config.deliveries.endpoint =
                    Some(Endpoint { host: f::boxed(b"127.0.0.1:8080"), target: f::boxed(b"/hooks/1\r\n") });
            }
            _ => unreachable!("table cases"),
        }
        assert_eq!(Service::new(config, &f::limits(), 1).unwrap_err(), crate::Problem::Limits);
    }
}

#[test]
fn github_fake_delivery_payload_choice_reaches_the_dialects_oversize_path() {
    let mut config = f::config();
    config.deliveries.payload_size = 5000;
    let mut service = Service::new(config, &f::limits(), 1).unwrap();
    let (id, _) = service
        .release(
            Delivery::Issue {
                repository: 2,
                number: 3,
                body: skein_forge::forgejo::document::Body::Kept(f::boxed(b"hello")),
            },
            Some(1),
            wall(1200),
        )
        .unwrap();
    let target = github::request::encode(
        &github::request::Request::LogDelivery { id, event: Box::new([]) },
        &f::limits().github,
    )
    .unwrap()
    .target;
    let response = request(&mut service, &target, None, wall(1203));
    assert_eq!(response.status, 200);
    assert!(response.body.len() > usize::try_from(f::limits().github.body_bytes).unwrap());
    let mut small = f::limits().github;
    small.writer.length = 8192;
    let sent = github::server::encode(
        &Delivery::Issue {
            repository: 2,
            number: 3,
            body: skein_forge::forgejo::document::Body::Kept(f::boxed(&[b'x'; 5000])),
        },
        &github::limits::Limits { body_bytes: 6000, ..small },
    )
    .unwrap();
    let Delivery::Issue { body: skein_forge::forgejo::document::Body::Long(_), .. } =
        github::delivery::decode(sent.event, &sent.body, &f::limits().github).unwrap()
    else {
        panic!("oversize issue body")
    };
}

#[test]
fn github_fake_installation_lifecycle_releases_events() {
    let mut service = Service::new(f::config(), &f::limits(), 23).unwrap();
    service.poll(wall(1200));
    let store = service.installations_mut();
    let id = store
        .install(22, b"acme", crate::installations::Kind::Organization, crate::installations::Reach::All, wall(1200))
        .unwrap();
    assert!(store.suspend(id, true));
    assert!(store.suspend(id, false));
    assert!(store.set_reach(id, crate::installations::Reach::Listed(List::with_capacity(0))));
    assert!(store.rename(id, b"widgets"));
    assert!(store.remove(id));
    let mut bounds = f::limits();
    bounds.github.inner = 16;
    service.poll(wall(1203));
    let page = request(&mut service, b"/app/hook/deliveries?per_page=8", None, wall(1203));
    let listed = rows(&page);
    assert_eq!(listed.len(), 6);
    let expected = [
        Change::Deleted,
        Change::Renamed { login: f::boxed(b"widgets") },
        Change::Repositories,
        Change::Unsuspended,
        Change::Suspended,
        Change::Created,
    ];
    for (row, change) in listed.iter().zip(&expected) {
        assert_eq!((row.installation_id, row.repository_id), (None, None));
        let target = github::request::encode(
            &github::request::Request::LogDelivery { id: row.id, event: row.event.clone() },
            &bounds.github,
        )
        .unwrap()
        .target;
        let reply = request(&mut service, &target, None, wall(1203));
        let (_, doc) = f::decode_with(
            github::decode::log_delivery_decoder(&row.event, &bounds.github),
            &reply.body,
            &bounds.github,
        );
        let Document::Delivery(logged) = doc else { panic!("whole delivery") };
        let Delivery::Installation { installation, account, change: found } = logged.delivery else {
            panic!("installation")
        };
        assert_eq!(installation, id);
        assert_eq!(account.id, 22);
        assert_eq!(&found, change);
        assert_eq!(row.delivered_at, wall(1200));
    }
}
