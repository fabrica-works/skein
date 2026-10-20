use crate::{
    Result,
    cases::Recorder,
    facts::{Case, Fact, add, case},
};
use skein_throwaway_forgejo::Token;
use std::path::Path;

/// Captures the admin's search after the connected listing provisions private repositories for three owners.
pub(crate) fn record(r: &mut Recorder<'_>, admin: &Token) -> Result<()> {
    for (name, page) in [("setup-search", 1), ("setup-search-page-2", 2), ("setup-search-repeat", 1)] {
        r.send(
            name,
            "GET",
            &format!("/api/v1/repos/search?sort=id&order=asc&page={page}&limit=1"),
            None,
            Some(admin),
            None,
        )?;
    }
    // The fixture caps a response at two rows, so read through the empty page.
    let mut page = 1_u32;
    loop {
        let response = r.send(
            &format!("setup-search-all-{page}"),
            "GET",
            &format!("/api/v1/repos/search?sort=id&order=asc&page={page}&limit=50"),
            None,
            Some(admin),
            None,
        )?;
        let body = crate::json(&response)?;
        let rows = body.root().get("data").array().ok_or("search data absent")?;
        if rows.is_empty() {
            break;
        }
        page = page.checked_add(1).ok_or("search page overflow")?;
    }
    Ok(())
}

fn listing(out: &Path) -> Result<Vec<Case>> {
    let mut pages = Vec::new();
    let mut page = 1_u32;
    loop {
        let response = case(out, &format!("setup-search-all-{page}"))?;
        let end = response.body.root().get("data").array().is_none_or(|rows| rows.is_empty());
        pages.push(response);
        if end {
            break;
        }
        page = page.checked_add(1).ok_or("search page overflow")?;
    }
    Ok(pages)
}

/// Evaluates private membership, the answer envelope and ascending numbered pages without recording a live forge.
pub(crate) fn facts(out: &Path) -> Result<Vec<Fact>> {
    let first = case(out, "setup-search")?;
    let second = case(out, "setup-search-page-2")?;
    let repeat = case(out, "setup-search-repeat")?;
    let all = listing(out)?;
    let envelope = [&first, &second, &repeat].into_iter().chain(all.iter()).all(|page| {
        page.status == 200
            && page.body.root().get("ok").bool() == Some(true)
            && page.body.root().get("data").array().is_some()
    });
    let rows = all
        .iter()
        .map(|page| page.body.root().get("data").array())
        .collect::<Option<Vec<_>>>()
        .map(|pages| pages.into_iter().flatten().collect::<Vec<_>>());
    let listed = envelope
        && rows.as_ref().is_some_and(|rows| {
            ["alice", "bob", "acme"].iter().all(|owner| {
                rows.iter().any(|row| {
                    row.get("owner").get("login").is(owner)
                        && row.get("owner").get("id").u64().is_some()
                        && row.get("name").is("widgets")
                        && row.get("private").bool() == Some(true)
                        && row.get("id").u64().is_some()
                })
            })
        });
    let ids = rows.as_ref().and_then(|rows| rows.iter().map(|row| row.get("id").u64()).collect::<Option<Vec<_>>>());
    let pages = listed
        && [&first, &second, &repeat]
            .iter()
            .all(|page| page.body.root().get("data").array().is_some_and(|rows| rows.len() == 1))
        && all.last().is_some_and(|page| page.body.root().get("data").array().is_some_and(|rows| rows.is_empty()))
        && ids.as_ref().is_some_and(|ids| {
            ids.len() >= 2
                && ids.windows(2).all(|pair| pair[0] < pair[1])
                && first.body.root().get("data").nth(0).get("id").u64() == ids.first().copied()
                && second.body.root().get("data").nth(0).get("id").u64() == ids.get(1).copied()
                && repeat.body.root().get("data").nth(0).get("id").u64() == ids.first().copied()
        });
    let mut facts = Vec::new();
    for (name, holds) in [
        ("setup-search-by-an-admin-lists-every-owners-private-repositories", listed),
        ("setup-search-answers-ok-and-data", envelope),
        ("setup-search-sorted-by-id-pages-by-page-and-limit", pages),
    ] {
        add(&mut facts, name, holds, "captured admin repository membership, envelope or pages differ");
    }
    Ok(facts)
}
