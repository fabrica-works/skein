//! A call follows a job log once to a configured, separately pooled origin (4.5).
use super::{Component, Waiting, github_calls::github_plan};
use crate::{Event, Fact, Limits};
use skein_forge::{
    api,
    github::{
        plans::plan::{Exchange, Retention},
        redirect::Redirect,
    },
};
use skein_lib::{Env, Id, Queue, Token};

impl Component {
    pub(super) fn follow_github_log(
        &mut self,
        holder: (Token, Id<Waiting>),
        redirect: Redirect,
        retention: Retention,
        env: &Env<Limits>,
        up: &mut Queue<Event>,
        io: &mut Queue<skein_io::Request>,
    ) {
        let (call, id) = holder;
        let number = self.waiting.get(id).expect("call").repository.forge;
        let mut position = None;
        for at in 0..self.settings.forges.len() {
            if self.settings.forges.get(at).expect("forge").number == number {
                position = Some(at);
                break;
            }
        }
        let at = position.expect("call forge");
        let forge = self.settings.forges.get(at).expect("forge");
        let mut matched = None;
        for index in 0..forge.log_origins.len() {
            if forge.log_origins.get(index).expect("origin").origin.same(&redirect.origin) {
                matched = Some(index);
                break;
            }
        }
        if let Some(token) = self.waiting.get(id).expect("call").connection {
            let c =
                self.connection_slab.get_mut(*self.connections.get(&token).expect("connection")).expect("connection");
            c.discard_redirect(env);
        }
        self.release_github_call(id, env, io);
        let Some(index) = matched else {
            let gh = self.github.get_mut(at).expect("forge").as_mut().expect("GitHub");
            let mut said = false;
            for origin in &gh.said_origins {
                if origin.same(&redirect.origin) {
                    said = true;
                    break;
                }
            }
            if !said {
                if gh.said_origins.room() == 0 {
                    drop(gh.said_origins.pop());
                }
                gh.said_origins.push(redirect.origin.clone());
                up.push(Event::Fact { forge: number, fact: Fact::Origin { origin: redirect.origin } });
            }
            self.finish(call, id, Err(api::Error::Refused), up);
            return;
        };
        let waiting = self.waiting.get_mut(id).expect("call");
        waiting.route.as_mut().expect("sent route").origin = Some(index);
        github_plan(waiting).set_pending(Exchange {
            request: skein_forge::github::redirect::request(&redirect),
            retention: Some(retention),
            comparison: skein_forge::github::plans::plan::Comparison::Full,
        });
        self.wake = Some(env.now);
    }
}
